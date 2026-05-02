#[cfg(target_os = "macos")]
pub mod darwin;
#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub mod other;
#[cfg(target_os = "windows")]
pub mod windows;

use crate::options::Options;
use crate::types::ProxyConfig;
use anyhow::Result;

pub fn disable_proxy(opt: Option<&Options>) -> Result<()> {
    #[cfg(target_os = "linux")]
    return linux::disable_proxy(opt);
    #[cfg(target_os = "macos")]
    return darwin::disable_proxy(opt);
    #[cfg(target_os = "windows")]
    return windows::disable_proxy(opt);
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    return other::disable_proxy(opt);
}

pub fn set_proxy(opt: Option<&Options>) -> Result<()> {
    #[cfg(target_os = "linux")]
    return linux::set_proxy(opt);
    #[cfg(target_os = "macos")]
    return darwin::set_proxy(opt);
    #[cfg(target_os = "windows")]
    return windows::set_proxy(opt);
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    return other::set_proxy(opt);
}

pub fn set_pac(opt: Option<&Options>) -> Result<()> {
    #[cfg(target_os = "linux")]
    return linux::set_pac(opt);
    #[cfg(target_os = "macos")]
    return darwin::set_pac(opt);
    #[cfg(target_os = "windows")]
    return windows::set_pac(opt);
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    return other::set_pac(opt);
}

pub fn query_proxy_settings(opt: Option<&Options>) -> Result<ProxyConfig> {
    #[cfg(target_os = "linux")]
    return linux::query_proxy_settings(opt);
    #[cfg(target_os = "macos")]
    return darwin::query_proxy_settings(opt);
    #[cfg(target_os = "windows")]
    return windows::query_proxy_settings(opt);
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    return other::query_proxy_settings(opt);
}
