use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Result, anyhow};

use crate::options::Options;
use crate::platform::{query_proxy_settings, set_pac, set_proxy};
use crate::types::ProxyConfig;
#[cfg(not(target_os = "windows"))]
use crate::watch::wait_proxy_settings_change;
#[cfg(target_os = "windows")]
use crate::watch::windows::wait_proxy_settings_change_timeout;

#[derive(Clone, Copy)]
enum GuardMode {
    Proxy,
    Pac,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GuardEvent {
    Changed,
    Restored,
    RestoreFailed,
}

impl GuardEvent {
    pub fn as_str(self) -> &'static str {
        match self {
            GuardEvent::Changed => "changed",
            GuardEvent::Restored => "restored",
            GuardEvent::RestoreFailed => "restore_failed",
        }
    }

    pub fn message(self) -> &'static str {
        match self {
            GuardEvent::Changed => "检测到代理设置变更，正在恢复...",
            GuardEvent::Restored => "代理设置已恢复",
            GuardEvent::RestoreFailed => "代理设置恢复失败",
        }
    }
}

const GUARD_WATCH_RECHECK_INTERVAL: Duration = Duration::from_secs(2);

pub fn apply_guard_proxy_settings(opt: Option<&Options>) -> Result<()> {
    if opt.map(|o| !o.pac_url.is_empty()).unwrap_or(false) {
        set_pac(opt)
    } else {
        set_proxy(opt)
    }
}

fn guard_mode(opt: Option<&Options>) -> GuardMode {
    if opt.map(|o| !o.pac_url.is_empty()).unwrap_or(false) {
        GuardMode::Pac
    } else {
        GuardMode::Proxy
    }
}

fn proxy_matches_expected(
    mode: GuardMode,
    opt: Option<&Options>,
    expected: &ProxyConfig,
    current: &ProxyConfig,
) -> bool {
    match mode {
        GuardMode::Pac => {
            let expected_url = opt
                .and_then(|o| (!o.pac_url.is_empty()).then_some(o.pac_url.as_str()))
                .unwrap_or(expected.pac.url.as_str());
            let expected_pac_enable = opt
                .map(|o| !o.pac_url.is_empty())
                .unwrap_or(expected.pac.enable);

            current.pac.enable == expected_pac_enable
                && current.pac.url == expected_url
                && !current.proxy.enable
        }
        GuardMode::Proxy => {
            let expected_server = opt
                .and_then(|o| (!o.proxy.is_empty()).then_some(o.proxy.as_str()))
                .unwrap_or_else(|| first_proxy_server(expected));
            let expected_bypass = opt
                .and_then(|o| (!o.bypass.is_empty()).then_some(o.bypass.as_str()))
                .unwrap_or(expected.proxy.bypass.as_str());
            let expected_proxy_enable = opt
                .map(|o| !o.proxy.is_empty())
                .unwrap_or(expected.proxy.enable);

            current.proxy.enable == expected_proxy_enable
                && proxy_servers_match(expected_server, &current.proxy.servers)
                && current.proxy.bypass == expected_bypass
                && !current.pac.enable
        }
    }
}

fn first_proxy_server(config: &ProxyConfig) -> &str {
    config
        .proxy
        .servers
        .get("http_server")
        .or_else(|| config.proxy.servers.get("https_server"))
        .or_else(|| config.proxy.servers.get("socks_server"))
        .map(String::as_str)
        .unwrap_or_default()
}

fn proxy_servers_match(
    expected_server: &str,
    current: &std::collections::HashMap<String, String>,
) -> bool {
    if expected_server.is_empty() {
        return current.values().all(|server| server.is_empty());
    }

    current
        .get("http_server")
        .is_some_and(|server| server == expected_server)
}

fn guard_apply_options(
    mode: GuardMode,
    opt: Option<&Options>,
    expected: &ProxyConfig,
) -> Option<Options> {
    let mut apply_opt = opt.cloned()?;
    match mode {
        GuardMode::Pac => {
            apply_opt.proxy.clear();
            apply_opt.bypass.clear();
            if !apply_opt.pac_url.is_empty() {
                return Some(apply_opt);
            }
            if !expected.pac.url.is_empty() {
                apply_opt.pac_url = expected.pac.url.clone();
            }
        }
        GuardMode::Proxy => {
            apply_opt.pac_url.clear();
            if apply_opt.bypass.is_empty() {
                apply_opt.bypass = expected.proxy.bypass.clone();
            }
            if apply_opt.proxy.is_empty() {
                let proxy = first_proxy_server(expected);
                if !proxy.is_empty() {
                    apply_opt.proxy = proxy.to_string();
                }
            }
        }
    }
    Some(apply_opt)
}

fn ensure_guard_proxy_settings(
    mode: GuardMode,
    expected: &ProxyConfig,
    apply_opt: Option<&Options>,
    query_opt: Option<&Options>,
    on_event: Option<&dyn Fn(GuardEvent)>,
) -> Result<()> {
    let should_restore = match query_proxy_settings(query_opt) {
        Ok(current) => !proxy_matches_expected(mode, apply_opt, expected, &current),
        Err(_) => true,
    };
    if should_restore {
        if let Some(on_event) = on_event {
            on_event(GuardEvent::Changed);
        }
        if let Err(e) = apply_guard_proxy_settings(apply_opt) {
            if let Some(on_event) = on_event {
                on_event(GuardEvent::RestoreFailed);
            }
            return Err(e);
        }
        if let Some(on_event) = on_event {
            on_event(GuardEvent::Restored);
        }
    }
    Ok(())
}

fn wait_guard_proxy_settings_change(cancel: Arc<AtomicBool>, opt: Option<&Options>) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        wait_proxy_settings_change_timeout(cancel, opt, Some(GUARD_WATCH_RECHECK_INTERVAL))?;
        return Ok(());
    }

    #[cfg(not(target_os = "windows"))]
    {
        let wait_cancel = Arc::new(AtomicBool::new(false));
        let timed_out = Arc::new(AtomicBool::new(false));
        let monitor_cancel = Arc::clone(&wait_cancel);
        let monitor_timed_out = Arc::clone(&timed_out);
        let parent_cancel = Arc::clone(&cancel);

        let monitor = std::thread::spawn(move || {
            let start = std::time::Instant::now();
            loop {
                if monitor_cancel.load(Ordering::SeqCst) {
                    return;
                }
                if parent_cancel.load(Ordering::SeqCst) {
                    monitor_cancel.store(true, Ordering::SeqCst);
                    return;
                }
                if start.elapsed() >= GUARD_WATCH_RECHECK_INTERVAL {
                    monitor_timed_out.store(true, Ordering::SeqCst);
                    monitor_cancel.store(true, Ordering::SeqCst);
                    return;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        });

        let result = wait_proxy_settings_change(Arc::clone(&wait_cancel), opt);
        wait_cancel.store(true, Ordering::SeqCst);
        let _ = monitor.join();

        if cancel.load(Ordering::SeqCst) {
            return Err(anyhow!("cancelled"));
        }
        if timed_out.load(Ordering::SeqCst) {
            return Ok(());
        }
        result
    }
}

pub fn guard_proxy_settings_after_apply(
    cancel: Arc<AtomicBool>,
    opt: Option<&Options>,
) -> Result<()> {
    guard_proxy_settings_after_apply_with_events(cancel, opt, |_| {})
}

pub fn guard_proxy_settings_after_apply_with_events(
    cancel: Arc<AtomicBool>,
    opt: Option<&Options>,
    on_event: impl Fn(GuardEvent),
) -> Result<()> {
    let watch_opt = opt.map(|o| Options {
        device: o.device.clone(),
        only_active_device: o.only_active_device,
        peer_pid: o.peer_pid,
        peer_uid: o.peer_uid,
        peer_gid: o.peer_gid,
        environment: o.environment.clone(),
        use_registry: o.use_registry,
        ..Default::default()
    });

    let mode = guard_mode(opt);
    let expected = query_proxy_settings(watch_opt.as_ref())
        .map_err(|e| anyhow!("读取守护目标代理设置失败：{}", e))?;
    let apply_opt = guard_apply_options(mode, opt, &expected);

    loop {
        if cancel.load(Ordering::SeqCst) {
            return Err(anyhow!("cancelled"));
        }
        ensure_guard_proxy_settings(
            mode,
            &expected,
            apply_opt.as_ref().or(opt),
            watch_opt.as_ref(),
            Some(&on_event),
        )?;
        wait_guard_proxy_settings_change(Arc::clone(&cancel), watch_opt.as_ref())?;
    }
}

pub fn guard_proxy_settings(cancel: Arc<AtomicBool>, opt: Option<&Options>) -> Result<()> {
    apply_guard_proxy_settings(opt)?;
    guard_proxy_settings_after_apply(cancel, opt)
}
