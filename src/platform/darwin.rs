use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::process::Command;

use crate::options::Options;
use crate::types::{format_server, parse_server_string, ProxyConfig};

pub fn disable_proxy(opt: Option<&Options>) -> Result<()> {
    let services = get_target_services(opt)?;
    let commands: &[&[&str]] = &[
        &["-setautoproxystate", "off"],
        &["-setproxyautodiscovery", "off"],
        &["-setwebproxystate", "off"],
        &["-setsecurewebproxystate", "off"],
        &["-setsocksfirewallproxystate", "off"],
    ];
    apply_network_services(
        &services,
        commands,
        crate::options::resolve_concurrent(opt),
    )
}

pub fn set_proxy(opt: Option<&Options>) -> Result<()> {
    let mut proxy = opt.map(|o| o.proxy.clone()).unwrap_or_default();
    let mut bypass = opt.map(|o| o.bypass.clone()).unwrap_or_default();

    if proxy.is_empty() || bypass.is_empty() {
        let config = query_proxy_settings(opt)?;
        if proxy.is_empty() {
            proxy = config
                .proxy
                .servers
                .get("http_server")
                .cloned()
                .unwrap_or_default();
        }
        if bypass.is_empty() {
            bypass = config.proxy.bypass.clone();
        }
    }

    let addr = parse_server_string(&proxy);
    if addr.host.is_empty() || addr.port.is_empty() {
        return Err(anyhow!("invalid proxy address: {}", proxy));
    }

    let services = get_target_services(opt)?;

    let bypass_parts: Vec<String> = bypass.split(',').map(|s| s.to_string()).collect();

    // We build commands separately to handle bypass list ownership
    let host = addr.host.as_str();
    let port = addr.port.as_str();
    let commands_owned: Vec<Vec<String>> = vec![
        vec!["-setautoproxystate".into(), "off".into()],
        vec!["-setproxyautodiscovery".into(), "off".into()],
        vec!["-setwebproxy".into(), host.into(), port.into()],
        vec!["-setsecurewebproxy".into(), host.into(), port.into()],
        vec!["-setsocksfirewallproxy".into(), host.into(), port.into()],
        {
            let mut v = vec!["-setproxybypassdomains".to_string()];
            v.extend(bypass_parts.clone());
            v
        },
    ];

    apply_network_services_owned(
        &services,
        &commands_owned,
        crate::options::resolve_concurrent(opt),
    )
}

pub fn set_pac(opt: Option<&Options>) -> Result<()> {
    let mut pac_url = opt.map(|o| o.pac_url.clone()).unwrap_or_default();

    if pac_url.is_empty() {
        let config = query_proxy_settings(opt)?;
        pac_url = config.pac.url.clone();
    }

    let services = get_target_services(opt)?;
    let commands_owned: Vec<Vec<String>> = vec![
        vec!["-setwebproxystate".into(), "off".into()],
        vec!["-setsecurewebproxystate".into(), "off".into()],
        vec!["-setsocksfirewallproxystate".into(), "off".into()],
        vec!["-setautoproxyurl".into(), pac_url.clone()],
        vec!["-setautoproxystate".into(), "on".into()],
        vec!["-setproxyautodiscovery".into(), "on".into()],
    ];

    apply_network_services_owned(
        &services,
        &commands_owned,
        crate::options::resolve_concurrent(opt),
    )
}

pub fn query_proxy_settings(opt: Option<&Options>) -> Result<ProxyConfig> {
    let services = get_query_services(opt)?;
    let service = &services[0];

    let mut config = ProxyConfig::default();
    config.proxy.servers = HashMap::new();

    // PAC / auto proxy
    let output = Command::new("networksetup")
        .args(["-getautoproxyurl", service])
        .output()?;
    if output.status.success() {
        let text = String::from_utf8_lossy(&output.stdout);
        if text.contains("Enabled: Yes") {
            config.pac.enable = true;
            for line in text.lines() {
                if let Some(url) = line.strip_prefix("URL: ") {
                    config.pac.url = url.trim().to_string();
                    break;
                }
            }
        }
    }

    // HTTP proxy
    if let Some((host, port)) = parse_proxy_output(
        &Command::new("networksetup")
            .args(["-getwebproxy", service])
            .output()?,
    ) {
        config.proxy.enable = true;
        let addr = format_server(&host, &port);
        if !addr.is_empty() {
            config.proxy.servers.insert("http_server".into(), addr);
        }
    }

    // HTTPS proxy
    if let Some((host, port)) = parse_proxy_output(
        &Command::new("networksetup")
            .args(["-getsecurewebproxy", service])
            .output()?,
    ) {
        config.proxy.enable = true;
        let addr = format_server(&host, &port);
        if !addr.is_empty() {
            config.proxy.servers.insert("https_server".into(), addr);
        }
    }

    // SOCKS proxy
    if let Some((host, port)) = parse_proxy_output(
        &Command::new("networksetup")
            .args(["-getsocksfirewallproxy", service])
            .output()?,
    ) {
        config.proxy.enable = true;
        let addr = format_server(&host, &port);
        if !addr.is_empty() {
            config.proxy.servers.insert("socks_server".into(), addr);
        }
    }

    // Bypass domains
    let output = Command::new("networksetup")
        .args(["-getproxybypassdomains", service])
        .output()?;
    if output.status.success() {
        let bypass = String::from_utf8_lossy(&output.stdout)
            .trim()
            .replace('\n', ",");
        if !bypass.is_empty() {
            config.proxy.bypass = bypass;
        }
    }

    Ok(config)
}

fn get_target_services(opt: Option<&Options>) -> Result<Vec<String>> {
    if let Some(opt) = opt {
        if !opt.device.is_empty() {
            return Ok(vec![opt.device.clone()]);
        }
    }
    let only_active = opt.map(|o| o.only_active_device).unwrap_or(false);
    get_network_services(only_active)
}

fn get_query_services(opt: Option<&Options>) -> Result<Vec<String>> {
    if let Some(opt) = opt {
        if !opt.device.is_empty() {
            return Ok(vec![opt.device.clone()]);
        }
        return get_network_services(opt.only_active_device);
    }
    if let Ok(services) = get_network_services(true) {
        return Ok(services);
    }
    get_network_services(false)
}

fn get_network_services(only_active: bool) -> Result<Vec<String>> {
    // Collect active interface names if needed
    let active_ifaces: Vec<String> = if only_active {
        get_active_interface_names()?
    } else {
        vec![]
    };

    let output = Command::new("networksetup")
        .arg("-listnetworkserviceorder")
        .output()
        .map_err(|e| anyhow!("无法执行 networksetup 命令：{}", e))?;
    if output.is_empty() {
        return Err(anyhow!("networksetup 命令没有输出"));
    }
    if !output.status.success() {
        return Err(anyhow!(
            "networksetup 命令失败：{}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let ordinal_re = regex_ordinal();
    let device_re = regex_device();

    let mut services: Vec<String> = Vec::new();
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        if let Some(caps) = ordinal_re.captures(line) {
            let service = caps[1].trim().to_string();
            // Try to read device from next line
            let device = lines
                .peek()
                .and_then(|next| device_re.captures(next))
                .map(|caps| caps[1].trim().to_string())
                .unwrap_or_default();
            if lines.peek().map(|next| device_re.is_match(next)).unwrap_or(false) {
                lines.next();
            }

            if only_active {
                if active_ifaces.contains(&device) {
                    services.push(service);
                }
            } else {
                services.push(service);
            }
        }
    }

    if services.is_empty() {
        return Err(anyhow!("未找到活跃的网络服务"));
    }
    Ok(services)
}

fn get_active_interface_names() -> Result<Vec<String>> {
    // Use getifaddrs via nix
    let ifaces = nix::ifaddrs::getifaddrs()
        .map_err(|e| anyhow!("无法获取网络接口：{}", e))?;

    let mut names: Vec<String> = Vec::new();
    for iface in ifaces {
        use nix::net::if_::InterfaceFlags;
        if iface.flags.contains(InterfaceFlags::IFF_UP)
            && iface.flags.contains(InterfaceFlags::IFF_RUNNING)
        {
            if let Some(_) = iface.address {
                let name = iface.interface_name.clone();
                if !names.contains(&name) {
                    names.push(name);
                }
            }
        }
    }
    Ok(names)
}

fn apply_network_services(
    services: &[String],
    commands: &[&[&str]],
    concurrent: bool,
) -> Result<()> {
    let owned: Vec<Vec<String>> = commands
        .iter()
        .map(|cmd| cmd.iter().map(|s| s.to_string()).collect())
        .collect();
    apply_network_services_owned(services, &owned, concurrent)
}

fn apply_network_services_owned(
    services: &[String],
    commands: &[Vec<String>],
    concurrent: bool,
) -> Result<()> {
    if !concurrent {
        for service in services {
            exec_networksetup_serial(service, commands)?;
        }
        return Ok(());
    }

    use std::sync::{Arc, Mutex};
    let error: Arc<Mutex<Option<anyhow::Error>>> = Arc::new(Mutex::new(None));
    let mut handles = vec![];

    for service in services {
        let service = service.clone();
        let commands: Vec<Vec<String>> = commands.to_vec();
        let error = Arc::clone(&error);
        let handle = std::thread::spawn(move || {
            if let Err(e) = exec_networksetup_concurrent(&service, &commands) {
                let mut lock = error.lock().unwrap();
                if lock.is_none() {
                    *lock = Some(e);
                }
            }
        });
        handles.push(handle);
    }

    for h in handles {
        let _ = h.join();
    }

    let lock = error.lock().unwrap();
    if let Some(e) = lock.as_ref() {
        return Err(anyhow!("{}", e));
    }
    Ok(())
}

fn exec_networksetup_serial(service: &str, commands: &[Vec<String>]) -> Result<()> {
    for cmd in commands {
        let mut args = vec![cmd[0].as_str(), service];
        let rest: Vec<&str> = cmd[1..].iter().map(String::as_str).collect();
        args.extend_from_slice(&rest);
        let status = Command::new("networksetup")
            .args(&args)
            .status()
            .map_err(|e| {
                anyhow!("执行 networksetup {:?} 时出错，服务 {}: {}", cmd, service, e)
            })?;
        if !status.success() {
            return Err(anyhow!(
                "执行 networksetup {:?} 时出错，服务 {}",
                cmd,
                service
            ));
        }
    }
    Ok(())
}

fn exec_networksetup_concurrent(service: &str, commands: &[Vec<String>]) -> Result<()> {
    use std::sync::{Arc, Mutex};
    let error: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let mut handles = vec![];

    for cmd in commands {
        let service = service.to_string();
        let cmd = cmd.clone();
        let error = Arc::clone(&error);
        let handle = std::thread::spawn(move || {
            let mut args = vec![cmd[0].as_str(), service.as_str()];
            let rest: Vec<&str> = cmd[1..].iter().map(String::as_str).collect();
            args.extend_from_slice(&rest);
            let status = Command::new("networksetup").args(&args).status();
            match status {
                Err(e) => {
                    let mut lock = error.lock().unwrap();
                    if lock.is_none() {
                        *lock = Some(format!(
                            "执行 networksetup {:?} 时出错，服务 {}: {}",
                            cmd, service, e
                        ));
                    }
                }
                Ok(s) if !s.success() => {
                    let mut lock = error.lock().unwrap();
                    if lock.is_none() {
                        *lock = Some(format!(
                            "执行 networksetup {:?} 时出错，服务 {}",
                            cmd, service
                        ));
                    }
                }
                _ => {}
            }
        });
        handles.push(handle);
    }

    for h in handles {
        let _ = h.join();
    }

    let lock = error.lock().unwrap();
    if let Some(msg) = lock.as_ref() {
        return Err(anyhow!("{}", msg));
    }
    Ok(())
}

/// 解析 networksetup -get*proxy 命令输出，返回 (host, port) 或 None（未启用）
fn parse_proxy_output(
    output: &std::process::Output,
) -> Option<(String, String)> {
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut enabled = false;
    let mut host = String::new();
    let mut port = String::new();

    for line in text.lines() {
        if line.starts_with("Enabled: Yes") {
            enabled = true;
        } else if let Some(h) = line.strip_prefix("Server: ") {
            host = h.trim().to_string();
        } else if let Some(p) = line.strip_prefix("Port: ") {
            port = p.trim().to_string();
        }
    }

    if enabled {
        Some((host, port))
    } else {
        None
    }
}

// Simple inline regex helpers to avoid regex crate dependency
fn regex_ordinal() -> SimpleRegex {
    SimpleRegex::Ordinal
}
fn regex_device() -> SimpleRegex {
    SimpleRegex::Device
}

enum SimpleRegex {
    Ordinal,
    Device,
}

impl SimpleRegex {
    fn captures<'a>(&self, line: &'a str) -> Option<Vec<&'a str>> {
        match self {
            SimpleRegex::Ordinal => {
                // Matches: "(N) Service Name"
                let line = line.trim();
                if line.starts_with('(') {
                    let end = line.find(')')?;
                    let num_str = &line[1..end];
                    if num_str.chars().all(|c| c.is_ascii_digit()) {
                        let service = line[end + 1..].trim();
                        return Some(vec![line, service]);
                    }
                }
                None
            }
            SimpleRegex::Device => {
                // Matches: "(Hardware Port: ..., Device: eth0, ...)"
                if let Some(pos) = line.find("Device: ") {
                    let rest = &line[pos + 8..];
                    let end = rest.find(|c: char| c == ',' || c == ')').unwrap_or(rest.len());
                    let device = rest[..end].trim();
                    return Some(vec![line, device]);
                }
                None
            }
        }
    }

    fn is_match(&self, line: &str) -> bool {
        self.captures(line).is_some()
    }
}

// Trait extension for Output
trait OutputExt {
    fn is_empty(&self) -> bool;
}
impl OutputExt for std::process::Output {
    fn is_empty(&self) -> bool {
        self.stdout.is_empty() && self.stderr.is_empty()
    }
}
