use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 代理配置
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProxyConfig {
    pub proxy: ProxyInfo,
    pub pac: PacInfo,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProxyInfo {
    pub enable: bool,
    pub same_for_all: bool,
    pub servers: HashMap<String, String>,
    pub bypass: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PacInfo {
    pub enable: bool,
    pub url: String,
}

/// 解析后的服务器地址
#[derive(Debug, Clone, Default)]
pub struct ServerAddr {
    pub host: String,
    pub port: String,
}

/// 格式化服务器地址为 "host:port" 字符串
pub fn format_server(host: &str, port: &str) -> String {
    let host = clean_output(host);
    let port = clean_output(port);

    if host.is_empty() || port.is_empty() || port == "0" {
        return String::new();
    }
    format!("{}:{}", host, port)
}

/// 去除字符串两端的引号、空白等
pub fn clean_output(s: &str) -> String {
    let s = s.trim_matches(|c: char| matches!(c, '\'' | '[' | ']' | '"' | ' ' | '\n'));
    s.trim().to_string()
}

/// 解析 "host:port" 格式的服务器字符串
pub fn parse_server_string(server: &str) -> ServerAddr {
    if server.is_empty() {
        return ServerAddr::default();
    }
    match server.rfind(':') {
        None => ServerAddr::default(),
        Some(idx) => ServerAddr {
            host: server[..idx].to_string(),
            port: server[idx + 1..].to_string(),
        },
    }
}
