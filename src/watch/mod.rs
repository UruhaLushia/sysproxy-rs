#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod darwin;
#[cfg(target_os = "windows")]
pub mod windows;
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub mod other;

use anyhow::Result;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use crate::options::Options;

/// 阻塞等待系统代理设置发生变更。
/// 当 `cancel` 被设置为 true 时，函数返回 Err（包含 "cancelled"）。
pub fn wait_proxy_settings_change(
    cancel: Arc<AtomicBool>,
    opt: Option<&Options>,
) -> Result<()> {
    #[cfg(target_os = "linux")]
    return linux::wait_proxy_settings_change(cancel, opt);
    #[cfg(target_os = "macos")]
    return darwin::wait_proxy_settings_change(cancel, opt);
    #[cfg(target_os = "windows")]
    return windows::wait_proxy_settings_change(cancel, opt);
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    return other::wait_proxy_settings_change(cancel, opt);
}
