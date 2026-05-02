#![deny(clippy::all)]

use napi::bindgen_prelude::*;
use napi_derive::napi;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

use sysproxy::{
    apply_guard_proxy_settings as _apply_guard_proxy_settings, disable_proxy as _disable_proxy,
    guard_proxy_settings_after_apply as _guard_proxy_settings_after_apply,
    query_proxy_settings as _query_proxy_settings, set_pac as _set_pac, set_proxy as _set_proxy,
    wait_proxy_settings_change as _wait_proxy_settings_change, Options,
};

// ── 暴露给 JS 的类型 ──────────────────────────────────────────────────────────

#[napi(object)]
pub struct JsOptions {
    /// 代理服务器地址（host:port），空字符串表示不设置
    pub proxy: Option<String>,
    /// 绕过地址列表，逗号分隔
    pub bypass: Option<String>,
    /// PAC 脚本 URL
    pub pac_url: Option<String>,
    /// 指定网络设备/连接名称
    pub device: Option<String>,
    /// 仅对活跃的网络设备生效
    pub only_active_device: Option<bool>,
    /// 是否并发执行；null 表示使用平台默认
    pub concurrent: Option<bool>,
    /// Windows：使用注册表而非 Win32 API
    pub use_registry: Option<bool>,
}

#[napi(object)]
#[derive(Default)]
pub struct JsProxyInfo {
    pub enable: bool,
    pub same_for_all: bool,
    /// JSON 序列化的 servers map（key: 协议名，value: host:port）
    pub servers: String,
    pub bypass: String,
}

#[napi(object)]
#[derive(Default)]
pub struct JsPacInfo {
    pub enable: bool,
    pub url: String,
}

#[napi(object)]
#[derive(Default)]
pub struct JsProxyConfig {
    pub proxy: JsProxyInfo,
    pub pac: JsPacInfo,
}

// ── 辅助：JsOptions → Options ─────────────────────────────────────────────────

fn to_options(js: Option<JsOptions>) -> Options {
    let Some(js) = js else {
        return Options::default();
    };
    Options {
        proxy: js.proxy.unwrap_or_default(),
        bypass: js.bypass.unwrap_or_default(),
        pac_url: js.pac_url.unwrap_or_default(),
        device: js.device.unwrap_or_default(),
        only_active_device: js.only_active_device.unwrap_or(false),
        concurrent: js.concurrent,
        use_registry: js.use_registry.unwrap_or(false),
        ..Options::default()
    }
}

fn map_err(e: anyhow::Error) -> napi::Error {
    napi::Error::from_reason(e.to_string())
}

fn is_cancelled_error(e: &anyhow::Error) -> bool {
    e.to_string().contains("cancelled")
}

fn join_guard_handle(handle: JoinHandle<anyhow::Result<()>>) -> Result<()> {
    match handle.join() {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) if is_cancelled_error(&e) => Ok(()),
        Ok(Err(e)) => Err(map_err(e)),
        Err(_) => Err(napi::Error::from_reason("proxy guard thread panicked")),
    }
}

// ── 导出函数 ──────────────────────────────────────────────────────────────────

/// 查询当前系统代理设置
#[napi]
pub fn query_proxy_settings(options: Option<JsOptions>) -> Result<JsProxyConfig> {
    let opt = to_options(options);
    let cfg = _query_proxy_settings(Some(&opt)).map_err(map_err)?;
    Ok(JsProxyConfig {
        proxy: JsProxyInfo {
            enable: cfg.proxy.enable,
            same_for_all: cfg.proxy.same_for_all,
            servers: serde_json::to_string(&cfg.proxy.servers).unwrap_or_default(),
            bypass: cfg.proxy.bypass,
        },
        pac: JsPacInfo {
            enable: cfg.pac.enable,
            url: cfg.pac.url,
        },
    })
}

/// 设置系统代理（HTTP/HTTPS/SOCKS）
#[napi]
pub fn set_proxy(options: Option<JsOptions>) -> Result<()> {
    let opt = to_options(options);
    _set_proxy(Some(&opt)).map_err(map_err)
}

/// 设置 PAC 自动代理
#[napi]
pub fn set_pac(options: Option<JsOptions>) -> Result<()> {
    let opt = to_options(options);
    _set_pac(Some(&opt)).map_err(map_err)
}

/// 取消系统代理设置
#[napi]
pub fn disable_proxy(options: Option<JsOptions>) -> Result<()> {
    let opt = to_options(options);
    _disable_proxy(Some(&opt)).map_err(map_err)
}

/// 阻塞等待一次系统代理设置变更
#[napi]
pub fn wait_proxy_settings_change(options: Option<JsOptions>) -> Result<()> {
    let opt = to_options(options);
    let cancel = Arc::new(AtomicBool::new(false));
    _wait_proxy_settings_change(cancel, Some(&opt)).map_err(map_err)
}

/// 在 Rust 后台线程中守护代理设置
#[napi]
pub struct ProxyGuard {
    options: Options,
    cancel: Option<Arc<AtomicBool>>,
    handle: Option<JoinHandle<anyhow::Result<()>>>,
}

#[napi]
impl ProxyGuard {
    #[napi(constructor)]
    pub fn new(options: Option<JsOptions>) -> Self {
        Self {
            options: to_options(options),
            cancel: None,
            handle: None,
        }
    }

    /// 应用代理设置并启动后台守护线程
    #[napi]
    pub fn start(&mut self) -> Result<()> {
        if let Some(handle) = self.handle.as_ref() {
            if !handle.is_finished() {
                return Err(napi::Error::from_reason("proxy guard is already running"));
            }
        }

        if let Some(handle) = self.handle.take() {
            join_guard_handle(handle)?;
        }
        self.cancel = None;

        let cancel = Arc::new(AtomicBool::new(false));
        let thread_cancel = Arc::clone(&cancel);
        let options = self.options.clone();
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);

        let handle = std::thread::spawn(move || {
            let apply_result = _apply_guard_proxy_settings(Some(&options));
            let _ = ready_tx.send(apply_result.as_ref().map(|_| ()).map_err(|e| e.to_string()));
            apply_result?;
            _guard_proxy_settings_after_apply(thread_cancel, Some(&options))
        });

        match ready_rx.recv() {
            Ok(Ok(())) => {
                self.cancel = Some(cancel);
                self.handle = Some(handle);
                Ok(())
            }
            Ok(Err(msg)) => {
                let _ = handle.join();
                Err(napi::Error::from_reason(msg))
            }
            Err(_) => {
                let _ = handle.join();
                Err(napi::Error::from_reason("proxy guard failed to start"))
            }
        }
    }

    /// 停止后台守护线程
    #[napi]
    pub fn stop(&mut self) -> Result<()> {
        if let Some(cancel) = self.cancel.take() {
            cancel.store(true, Ordering::SeqCst);
        }
        if let Some(handle) = self.handle.take() {
            join_guard_handle(handle)?;
        }
        Ok(())
    }

    /// 当前守护线程是否仍在运行
    #[napi]
    pub fn is_running(&self) -> bool {
        self.handle
            .as_ref()
            .map(|handle| !handle.is_finished())
            .unwrap_or(false)
    }
}

impl Drop for ProxyGuard {
    fn drop(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            cancel.store(true, Ordering::SeqCst);
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
