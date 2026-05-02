#![deny(clippy::all)]

use napi::bindgen_prelude::*;
use napi_derive::napi;

use sysproxy::{
    disable_proxy as _disable_proxy, query_proxy_settings as _query_proxy_settings,
    set_pac as _set_pac, set_proxy as _set_proxy, Options,
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
    /// JSON 序列化的 servers map（key: 协议名, value: host:port）
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
