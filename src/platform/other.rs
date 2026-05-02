use anyhow::{anyhow, Result};
use crate::options::Options;
use crate::types::ProxyConfig;

pub fn disable_proxy(_opt: Option<&Options>) -> Result<()> {
    Err(anyhow!("不支持的操作系统"))
}

pub fn set_proxy(_opt: Option<&Options>) -> Result<()> {
    Err(anyhow!("不支持的操作系统"))
}

pub fn set_pac(_opt: Option<&Options>) -> Result<()> {
    Err(anyhow!("不支持的操作系统"))
}

pub fn query_proxy_settings(_opt: Option<&Options>) -> Result<ProxyConfig> {
    Err(anyhow!("不支持的操作系统"))
}
