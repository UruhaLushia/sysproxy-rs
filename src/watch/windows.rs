use anyhow::{Result, anyhow};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::options::Options;

pub fn wait_proxy_settings_change(cancel: Arc<AtomicBool>, opt: Option<&Options>) -> Result<()> {
    wait_proxy_settings_change_timeout(cancel, opt, None).map(|_| ())
}

pub fn wait_proxy_settings_change_timeout(
    cancel: Arc<AtomicBool>,
    opt: Option<&Options>,
    timeout: Option<Duration>,
) -> Result<bool> {
    validate_registry_target(opt)?;

    use windows::Win32::Foundation::{ERROR_SUCCESS, HANDLE, WAIT_TIMEOUT};
    use windows::Win32::System::Registry::{
        HKEY, REG_NOTIFY_CHANGE_LAST_SET, REG_NOTIFY_CHANGE_NAME, REG_NOTIFY_THREAD_AGNOSTIC,
        RegNotifyChangeKeyValue,
    };
    use windows::Win32::System::Threading::{CreateEventW, WaitForMultipleObjects};
    use windows::core::PCWSTR;

    const WAIT_SLICE: Duration = Duration::from_millis(200);

    let paths = [
        r"Software\Microsoft\Windows\CurrentVersion\Internet Settings",
        r"Software\Microsoft\Windows\CurrentVersion\Internet Settings\Connections",
    ];

    let mut keys: Vec<HKEY> = Vec::new();
    let mut events: Vec<HANDLE> = Vec::new();

    for path in &paths {
        let key = open_current_user_key_notify(path)?;
        keys.push(key);
    }

    for key in &keys {
        let event = unsafe { CreateEventW(None, false, false, PCWSTR::null()) }
            .map_err(|e| anyhow!("创建代理设置变更事件失败：{}", e))?;
        events.push(event);

        let filter =
            REG_NOTIFY_CHANGE_LAST_SET | REG_NOTIFY_CHANGE_NAME | REG_NOTIFY_THREAD_AGNOSTIC;
        let result = unsafe { RegNotifyChangeKeyValue(*key, false, filter, Some(event), true) };
        if result != ERROR_SUCCESS {
            let filter2 = REG_NOTIFY_CHANGE_LAST_SET | REG_NOTIFY_CHANGE_NAME;
            let result2 =
                unsafe { RegNotifyChangeKeyValue(*key, false, filter2, Some(event), true) };
            if result2 != ERROR_SUCCESS {
                cleanup_keys_events(&keys, &events);
                return Err(anyhow!("监听代理设置注册表失败：{}", result2.0));
            }
        }
    }

    let start = Instant::now();
    let result = loop {
        if cancel.load(Ordering::SeqCst) {
            break Err(anyhow!("cancelled"));
        }

        let wait_ms = if let Some(limit) = timeout {
            let Some(remaining) = limit.checked_sub(start.elapsed()) else {
                break Ok(false);
            };
            if remaining.is_zero() {
                break Ok(false);
            }
            remaining.min(WAIT_SLICE).as_millis().max(1) as u32
        } else {
            WAIT_SLICE.as_millis() as u32
        };

        let wait_result = unsafe { WaitForMultipleObjects(&events, false, wait_ms) };
        if wait_result.0 == WAIT_TIMEOUT.0 {
            if timeout
                .map(|limit| start.elapsed() >= limit)
                .unwrap_or(false)
            {
                break Ok(false);
            }
            continue;
        }

        let idx = wait_result.0;
        if idx >= events.len() as u32 {
            break Err(anyhow!("等待代理设置变更失败：返回值 {}", idx));
        }
        break Ok(true);
    };

    cleanup_keys_events(&keys, &events);
    result
}

fn validate_registry_target(opt: Option<&Options>) -> Result<()> {
    if opt
        .map(|o| o.use_registry && !o.device.is_empty())
        .unwrap_or(false)
    {
        return Err(anyhow!("注册表模式不支持指定网络设备"));
    }
    Ok(())
}

fn open_current_user_key_notify(path: &str) -> Result<windows::Win32::System::Registry::HKEY> {
    use windows::Win32::System::Registry::{
        HKEY, KEY_NOTIFY, REG_SAM_FLAGS, RegOpenCurrentUser, RegOpenKeyExW,
    };
    use windows::core::PCWSTR;

    let path_wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let mut current_user = HKEY::default();
        let r = RegOpenCurrentUser(KEY_NOTIFY.0, &mut current_user);
        if r != windows::Win32::Foundation::ERROR_SUCCESS {
            return Err(anyhow!("RegOpenCurrentUser 失败：{}", r.0));
        }
        let mut key = HKEY::default();
        let r = RegOpenKeyExW(
            current_user,
            PCWSTR(path_wide.as_ptr()),
            None,
            REG_SAM_FLAGS(KEY_NOTIFY.0),
            &mut key,
        );
        let _ = windows::Win32::System::Registry::RegCloseKey(current_user);
        if r != windows::Win32::Foundation::ERROR_SUCCESS {
            return Err(anyhow!("RegOpenKeyExW({}) 失败：{}", path, r.0));
        }
        Ok(key)
    }
}

fn cleanup_keys_events(
    keys: &[windows::Win32::System::Registry::HKEY],
    events: &[windows::Win32::Foundation::HANDLE],
) {
    for &key in keys {
        unsafe {
            let _ = windows::Win32::System::Registry::RegCloseKey(key);
        };
    }
    for &ev in events {
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(ev);
        };
    }
}
