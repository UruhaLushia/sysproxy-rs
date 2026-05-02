pub mod context;

use anyhow::{Result, anyhow};
use std::collections::HashMap;

use crate::options::Options;
use crate::types::{ProxyConfig, clean_output, format_server, parse_server_string};
use context::LinuxExecContext;

/// 桌面环境检测结果
struct Environment {
    ctx: LinuxExecContext,
    desktop: String,
    is_kde: bool,
    is_kde6: bool,
    is_gnome: bool,
}

impl Environment {
    fn init(opt: Option<&Options>) -> Result<Self> {
        let ctx = LinuxExecContext::new(opt)?;
        let desktop = ctx
            .env_map
            .get("XDG_CURRENT_DESKTOP")
            .cloned()
            .unwrap_or_default();
        if desktop.is_empty() {
            return Err(anyhow!("XDG_CURRENT_DESKTOP environment variable not set"));
        }
        let is_kde = desktop == "KDE";
        let is_kde6 =
            is_kde && ctx.env_map.get("KDE_SESSION_VERSION").map(|s| s.as_str()) == Some("6");
        let is_gnome = desktop.contains("GNOME")
            || desktop == "Unity"
            || desktop == "X-Cinnamon"
            || desktop == "niri";

        Ok(Environment {
            ctx,
            desktop,
            is_kde,
            is_kde6,
            is_gnome,
        })
    }
}

pub fn disable_proxy(opt: Option<&Options>) -> Result<()> {
    let e = Environment::init(opt)?;
    if e.is_kde {
        clear_kde_proxy(&e)
    } else if e.is_gnome {
        clear_gnome_proxy(&e)
    } else {
        Err(anyhow!("不支持的桌面：{}", e.desktop))
    }
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

    let e = Environment::init(opt)?;
    let mut config = ProxyConfig::default();
    config.proxy.enable = true;
    config.proxy.same_for_all = true;
    config.proxy.servers = HashMap::from([
        ("http_server".into(), proxy.clone()),
        ("https_server".into(), proxy.clone()),
        ("socks_server".into(), proxy),
    ]);
    config.proxy.bypass = bypass;

    if e.is_kde {
        set_kde_proxy(&e, &config)
    } else if e.is_gnome {
        set_gnome_proxy(&e, &config)
    } else {
        Err(anyhow!("不支持的桌面：{}", e.desktop))
    }
}

pub fn set_pac(opt: Option<&Options>) -> Result<()> {
    let mut pac_url = opt.map(|o| o.pac_url.clone()).unwrap_or_default();
    let e = Environment::init(opt)?;

    if pac_url.is_empty() {
        let config = query_proxy_settings(opt)?;
        pac_url = config.pac.url.clone();
    }

    let mut config = ProxyConfig::default();
    config.pac.enable = true;
    config.pac.url = pac_url;

    if e.is_kde {
        set_kde_pac(&e, &config)
    } else if e.is_gnome {
        set_gnome_pac(&e, &config)
    } else {
        Err(anyhow!("不支持的桌面：{}", e.desktop))
    }
}

pub fn query_proxy_settings(opt: Option<&Options>) -> Result<ProxyConfig> {
    let e = Environment::init(opt)?;
    if e.is_kde {
        query_kde_settings(&e)
    } else if e.is_gnome {
        query_gnome_settings(&e)
    } else {
        Err(anyhow!("不支持的桌面：{}", e.desktop))
    }
}

// ── GNOME ──────────────────────────────────────────────────────────────────

fn query_gnome_settings(e: &Environment) -> Result<ProxyConfig> {
    let keys: &[(&str, &[&str])] = &[
        ("mode", &["org.gnome.system.proxy", "mode"]),
        ("ignore-hosts", &["org.gnome.system.proxy", "ignore-hosts"]),
        (
            "autoconfig-url",
            &["org.gnome.system.proxy", "autoconfig-url"],
        ),
        (
            "use-same-proxy",
            &["org.gnome.system.proxy", "use-same-proxy"],
        ),
        ("http_host", &["org.gnome.system.proxy.http", "host"]),
        ("http_port", &["org.gnome.system.proxy.http", "port"]),
        ("https_host", &["org.gnome.system.proxy.https", "host"]),
        ("https_port", &["org.gnome.system.proxy.https", "port"]),
        ("ftp_host", &["org.gnome.system.proxy.ftp", "host"]),
        ("ftp_port", &["org.gnome.system.proxy.ftp", "port"]),
        ("socks_host", &["org.gnome.system.proxy.socks", "host"]),
        ("socks_port", &["org.gnome.system.proxy.socks", "port"]),
    ];

    let mut settings: HashMap<String, String> = HashMap::new();
    for (name, path) in keys {
        let mut cmd_args = vec!["get"];
        cmd_args.extend_from_slice(path);
        let output = e
            .ctx
            .command("gsettings", &cmd_args)
            .output()
            .map_err(|e2| anyhow!("无法读取 {} 的 GNOME 配置：{}", name, e2))?;
        if !output.status.success() {
            return Err(anyhow!(
                "无法读取 {} 的 GNOME 配置：{}",
                name,
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        settings.insert(
            name.to_string(),
            String::from_utf8_lossy(&output.stdout).into_owned(),
        );
    }

    let mut config = ProxyConfig::default();
    config.proxy.enable =
        clean_output(settings.get("mode").map(String::as_str).unwrap_or("")) == "manual";
    config.proxy.same_for_all = clean_output(
        settings
            .get("use-same-proxy")
            .map(String::as_str)
            .unwrap_or(""),
    ) == "true";

    config.proxy.servers = HashMap::from([
        (
            "http_server".into(),
            format_server(
                settings.get("http_host").map(String::as_str).unwrap_or(""),
                settings.get("http_port").map(String::as_str).unwrap_or(""),
            ),
        ),
        (
            "https_server".into(),
            format_server(
                settings.get("https_host").map(String::as_str).unwrap_or(""),
                settings.get("https_port").map(String::as_str).unwrap_or(""),
            ),
        ),
        (
            "socks_server".into(),
            format_server(
                settings.get("socks_host").map(String::as_str).unwrap_or(""),
                settings.get("socks_port").map(String::as_str).unwrap_or(""),
            ),
        ),
        (
            "ftp_server".into(),
            format_server(
                settings.get("ftp_host").map(String::as_str).unwrap_or(""),
                settings.get("ftp_port").map(String::as_str).unwrap_or(""),
            ),
        ),
    ]);

    let bypass_raw = clean_output(
        settings
            .get("ignore-hosts")
            .map(String::as_str)
            .unwrap_or(""),
    );
    if !bypass_raw.is_empty() {
        let items: Vec<String> = bypass_raw
            .split(',')
            .map(|s| clean_output(s))
            .filter(|s| !s.is_empty())
            .collect();
        config.proxy.bypass = items.join(",");
    }

    config.pac.enable =
        clean_output(settings.get("mode").map(String::as_str).unwrap_or("")) == "auto";
    config.pac.url = clean_output(
        settings
            .get("autoconfig-url")
            .map(String::as_str)
            .unwrap_or(""),
    );

    Ok(config)
}

fn set_gnome_proxy(e: &Environment, config: &ProxyConfig) -> Result<()> {
    exec_gsettings(e, "org.gnome.system.proxy", "mode", "manual")?;

    let proxy_types = [
        ("http", "http_server"),
        ("https", "https_server"),
        ("ftp", "ftp_server"),
        ("socks", "socks_server"),
    ];

    for (proxy_type, key) in &proxy_types {
        let server = config
            .proxy
            .servers
            .get(*key)
            .map(String::as_str)
            .unwrap_or("");
        if server.is_empty() {
            continue;
        }
        let addr = parse_server_string(server);
        if addr.host.is_empty() {
            continue;
        }
        let schema = format!("org.gnome.system.proxy.{}", proxy_type);
        exec_gsettings(e, &schema, "host", &addr.host)?;
        exec_gsettings(e, &schema, "port", &addr.port)?;
    }

    if !config.proxy.bypass.is_empty() {
        let bypass_list = format!(
            "['{}']",
            config
                .proxy
                .bypass
                .split(',')
                .collect::<Vec<_>>()
                .join("','")
        );
        exec_gsettings(e, "org.gnome.system.proxy", "ignore-hosts", &bypass_list)?;
    }

    exec_gsettings(
        e,
        "org.gnome.system.proxy",
        "use-same-proxy",
        if config.proxy.same_for_all {
            "true"
        } else {
            "false"
        },
    )
}

fn set_gnome_pac(e: &Environment, config: &ProxyConfig) -> Result<()> {
    exec_gsettings(e, "org.gnome.system.proxy", "mode", "auto")?;
    exec_gsettings(
        e,
        "org.gnome.system.proxy",
        "autoconfig-url",
        &config.pac.url,
    )
}

fn clear_gnome_proxy(e: &Environment) -> Result<()> {
    exec_gsettings(e, "org.gnome.system.proxy", "mode", "none")
}

fn exec_gsettings(e: &Environment, schema: &str, key: &str, value: &str) -> Result<()> {
    let status = e
        .ctx
        .command("gsettings", &["set", schema, key, value])
        .status()
        .map_err(|e2| anyhow!("执行 gsettings 失败：{}", e2))?;
    if !status.success() {
        return Err(anyhow!("gsettings set {} {} {} 失败", schema, key, value));
    }
    Ok(())
}

// ── KDE ────────────────────────────────────────────────────────────────────

fn kde_cmd_names(e: &Environment) -> (&'static str, &'static str, &'static str) {
    if e.is_kde6 {
        ("kreadconfig6", "kwriteconfig6", "Proxy Settings")
    } else {
        ("kreadconfig5", "kwriteconfig5", "Proxy")
    }
}

fn query_kde_settings(e: &Environment) -> Result<ProxyConfig> {
    let (read_cmd, _, group) = kde_cmd_names(e);
    let keys = [
        "ProxyType",
        "httpProxy",
        "httpsProxy",
        "socksProxy",
        "ftpProxy",
        "NoProxyFor",
        "Proxy Config Script",
        "UseSameProxy",
    ];

    let mut values: HashMap<String, String> = HashMap::new();
    for key in &keys {
        let output = e
            .ctx
            .command(
                read_cmd,
                &["--file", "kioslaverc", "--group", group, "--key", key],
            )
            .output()
            .map_err(|e2| anyhow!("无法读取 {} 的 KDE 配置：{}", key, e2))?;
        values.insert(
            key.to_string(),
            clean_output(&String::from_utf8_lossy(&output.stdout)),
        );
    }

    let mut config = ProxyConfig::default();
    config.proxy.enable = values.get("ProxyType").map(String::as_str) == Some("1");
    config.proxy.same_for_all = values.get("UseSameProxy").map(String::as_str) == Some("true");

    config.proxy.servers = HashMap::from([
        (
            "http_server".into(),
            values
                .get("httpProxy")
                .map(|s| s.replace(' ', ":"))
                .unwrap_or_default(),
        ),
        (
            "https_server".into(),
            values
                .get("httpsProxy")
                .map(|s| s.replace(' ', ":"))
                .unwrap_or_default(),
        ),
        (
            "socks_server".into(),
            values
                .get("socksProxy")
                .map(|s| s.replace(' ', ":"))
                .unwrap_or_default(),
        ),
        (
            "ftp_server".into(),
            values
                .get("ftpProxy")
                .map(|s| s.replace(' ', ":"))
                .unwrap_or_default(),
        ),
    ]);

    // 清除空值
    for v in config.proxy.servers.values_mut() {
        if v.is_empty() || v == "0" {
            *v = String::new();
        }
    }

    config.proxy.bypass = values.get("NoProxyFor").cloned().unwrap_or_default();
    config.pac.enable = values.get("ProxyType").map(String::as_str) == Some("2");
    config.pac.url = values
        .get("Proxy Config Script")
        .cloned()
        .unwrap_or_default();

    Ok(config)
}

fn set_kde_proxy(e: &Environment, config: &ProxyConfig) -> Result<()> {
    let (_, write_cmd, group) = kde_cmd_names(e);
    exec_kde_config(e, write_cmd, "ProxyType", "1", group)?;

    let servers = [
        ("httpProxy", "http_server"),
        ("httpsProxy", "https_server"),
        ("socksProxy", "socks_server"),
        ("ftpProxy", "ftp_server"),
    ];
    for (kde_key, config_key) in &servers {
        let value = config
            .proxy
            .servers
            .get(*config_key)
            .map(String::as_str)
            .unwrap_or("");
        exec_kde_config(e, write_cmd, kde_key, value, group)?;
    }

    exec_kde_config(e, write_cmd, "NoProxyFor", &config.proxy.bypass, group)?;
    exec_kde_config(
        e,
        write_cmd,
        "UseSameProxy",
        if config.proxy.same_for_all {
            "true"
        } else {
            "false"
        },
        group,
    )
}

fn set_kde_pac(e: &Environment, config: &ProxyConfig) -> Result<()> {
    let (_, write_cmd, group) = kde_cmd_names(e);
    exec_kde_config(e, write_cmd, "ProxyType", "2", group)?;
    exec_kde_config(e, write_cmd, "Proxy Config Script", &config.pac.url, group)
}

fn clear_kde_proxy(e: &Environment) -> Result<()> {
    let (_, write_cmd, group) = kde_cmd_names(e);
    exec_kde_config(e, write_cmd, "ProxyType", "0", group)
}

fn exec_kde_config(e: &Environment, cmd: &str, key: &str, value: &str, group: &str) -> Result<()> {
    let status = e
        .ctx
        .command(
            cmd,
            &[
                "--file",
                "kioslaverc",
                "--group",
                group,
                "--key",
                key,
                value,
            ],
        )
        .status()
        .map_err(|e2| anyhow!("执行 {} 失败：{}", cmd, e2))?;
    if !status.success() {
        return Err(anyhow!("{} --key {} 失败", cmd, key));
    }
    Ok(())
}
