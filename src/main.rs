use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{anyhow, Result};

use sysproxy::{
    default_concurrent, disable_proxy, query_proxy_settings, set_pac, set_proxy,
    wait_proxy_settings_change, Options,
};

// ── CLI 数据结构 ──────────────────────────────────────────────────────────────

struct Cli {
    only_active_device: bool,
    device: String,
    multithread: bool,
    registry: bool,
    command: Commands,
}

enum Commands {
    Proxy { server: String, bypass: String },
    Pac { url: String },
    Disable,
    Status,
    Watch,
    Guard { server: String, bypass: String, url: String },
}

// ── 参数解析 ──────────────────────────────────────────────────────────────────

fn parse_args() -> Result<Cli> {
    let mut args = pico_args::Arguments::from_env();

    if args.contains(["-h", "--help"]) {
        print_help();
        std::process::exit(0);
    }

    let only_active_device = args.contains(["-a", "--only-active-device"]);
    let registry = args.contains("--registry");
    let device: String = args.opt_value_from_str(["-d", "--device"])?.unwrap_or_default();
    let multithread = if args.contains("--multithread") {
        true
    } else if args.contains("--no-multithread") {
        false
    } else {
        default_concurrent()
    };

    let subcmd: String = args.free_from_str().map_err(|_| {
        print_help();
        anyhow!("未指定子命令")
    })?;

    let command = match subcmd.as_str() {
        "proxy" => Commands::Proxy {
            server: args.opt_value_from_str(["-s", "--server"])?.unwrap_or_default(),
            bypass: args.opt_value_from_str(["-b", "--bypass"])?.unwrap_or_default(),
        },
        "pac" => Commands::Pac {
            url: args.opt_value_from_str(["-u", "--url"])?.unwrap_or_default(),
        },
        "disable" => Commands::Disable,
        "status"  => Commands::Status,
        "watch"   => Commands::Watch,
        "guard" => Commands::Guard {
            server: args.opt_value_from_str(["-s", "--server"])?.unwrap_or_default(),
            bypass: args.opt_value_from_str(["-b", "--bypass"])?.unwrap_or_default(),
            url:    args.opt_value_from_str(["-u", "--url"])?.unwrap_or_default(),
        },
        other => return Err(anyhow!("未知子命令：{}", other)),
    };

    Ok(Cli { only_active_device, device, multithread, registry, command })
}

fn print_help() {
    eprintln!("用法：sysproxy [选项] <子命令> [子命令选项]");
    eprintln!();
    eprintln!("选项：");
    eprintln!("  -a, --only-active-device      仅对活跃的网络设备生效");
    eprintln!("  -d, --device <设备>            指定网络设备");
    eprintln!("      --multithread              启用多线程并发设置");
    eprintln!("      --no-multithread           禁用多线程并发设置");
    eprintln!("      --registry                 Windows 使用注册表设置/查询代理");
    eprintln!();
    eprintln!("子命令：");
    eprintln!("  proxy  -s <服务器> [-b <绕过>]              设置系统代理");
    eprintln!("  pac    -u <PAC 地址>                         设置 PAC 代理");
    eprintln!("  disable                                      取消代理设置");
    eprintln!("  status                                       查看当前代理设置");
    eprintln!("  watch                                        监听系统代理设置变更");
    eprintln!("  guard  -s <服务器> [-b <绕过>] [-u <PAC>]  守护系统代理设置");
}

// ── 入口 ──────────────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    let cli = parse_args()?;

    macro_rules! opts {
        () => {
            Options {
                device: cli.device.clone(),
                only_active_device: cli.only_active_device,
                concurrent: Some(cli.multithread),
                use_registry: cli.registry,
                ..Default::default()
            }
        };
        ($($field:ident : $val:expr),+ $(,)?) => {{
            let mut opt = opts!();
            $(opt.$field = $val;)+
            opt
        }};
    }

    match &cli.command {
        Commands::Proxy { server, bypass } => {
            let t = Instant::now();
            let opt = opts!(proxy: server.clone(), bypass: bypass.clone());
            set_proxy(Some(&opt))?;
            println!("代理设置成功，耗时：{:?}", t.elapsed());
        }

        Commands::Pac { url } => {
            let t = Instant::now();
            let opt = opts!(pac_url: url.clone());
            set_pac(Some(&opt))?;
            println!("PAC 代理设置成功，耗时：{:?}", t.elapsed());
        }

        Commands::Disable => {
            let t = Instant::now();
            let opt = opts!();
            disable_proxy(Some(&opt))?;
            println!("代理设置已取消，耗时：{:?}", t.elapsed());
        }

        Commands::Status => {
            let opt = opts!();
            let status = query_proxy_settings(Some(&opt))?;
            println!("{}", serde_json::to_string_pretty(&status)?);
        }

        Commands::Watch => {
            let opt = opts!();
            let cancel = Arc::new(AtomicBool::new(false));
            let cancel_clone = Arc::clone(&cancel);
            ctrlc::set_handler(move || {
                cancel_clone.store(true, Ordering::SeqCst);
            })
            .ok();

            loop {
                match wait_proxy_settings_change(Arc::clone(&cancel), Some(&opt)) {
                    Ok(()) => println!("update"),
                    Err(e) if e.to_string().contains("cancelled") => break,
                    Err(e) => {
                        eprintln!("监听代理设置失败：{}", e);
                        break;
                    }
                }
            }
        }

        Commands::Guard { server, bypass, url } => {
            let opt = opts!(proxy: server.clone(), bypass: bypass.clone(), pac_url: url.clone());
            let watch_opt = opts!();

            let apply: Box<dyn Fn() -> Result<()>> = if !url.is_empty() {
                Box::new(|| set_pac(Some(&opt)))
            } else {
                Box::new(|| set_proxy(Some(&opt)))
            };

            apply()?;
            println!("代理已设置，开始守护...");

            let cancel = Arc::new(AtomicBool::new(false));
            let cancel_clone = Arc::clone(&cancel);
            ctrlc::set_handler(move || {
                cancel_clone.store(true, Ordering::SeqCst);
            })
            .ok();

            loop {
                match wait_proxy_settings_change(Arc::clone(&cancel), Some(&watch_opt)) {
                    Ok(()) => {
                        println!("检测到代理设置变更，正在恢复...");
                        match apply() {
                            Ok(()) => println!("代理设置已恢复"),
                            Err(e) => eprintln!("恢复代理设置失败：{}", e),
                        }
                    }
                    Err(e) if e.to_string().contains("cancelled") => break,
                    Err(e) => {
                        eprintln!("监听代理设置失败：{}", e);
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}
