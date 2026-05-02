use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use anyhow::Result;

use crate::options::Options;
use crate::platform::{set_pac, set_proxy};
use crate::watch::wait_proxy_settings_change;

/// 应用需要守护的代理设置。
///
/// `pac_url` 非空时设置 PAC，否则设置普通代理。
pub fn apply_guard_proxy_settings(opt: Option<&Options>) -> Result<()> {
    if opt.map(|o| !o.pac_url.is_empty()).unwrap_or(false) {
        set_pac(opt)
    } else {
        set_proxy(opt)
    }
}

/// 在代理设置已经应用后，阻塞守护代理配置。
///
/// 当检测到系统代理设置变化时，会重新应用 `opt` 指定的代理配置。
pub fn guard_proxy_settings_after_apply(
    cancel: Arc<AtomicBool>,
    opt: Option<&Options>,
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

    loop {
        wait_proxy_settings_change(Arc::clone(&cancel), watch_opt.as_ref())?;
        apply_guard_proxy_settings(opt)?;
    }
}

/// 应用并守护代理设置，直到 `cancel` 被置为 true 或监听/恢复失败。
pub fn guard_proxy_settings(cancel: Arc<AtomicBool>, opt: Option<&Options>) -> Result<()> {
    apply_guard_proxy_settings(opt)?;
    guard_proxy_settings_after_apply(cancel, opt)
}
