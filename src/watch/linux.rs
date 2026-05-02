use anyhow::{Result, anyhow};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::options::Options;
use crate::platform::linux::context::LinuxExecContext;

/// 等待系统代理设置变更。
/// `cancel` 置为 true 时函数尽快返回 Err("cancelled")。
pub fn wait_proxy_settings_change(cancel: Arc<AtomicBool>, opt: Option<&Options>) -> Result<()> {
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
    let is_gnome = desktop.contains("GNOME")
        || desktop == "Unity"
        || desktop == "X-Cinnamon"
        || desktop == "niri";

    if is_gnome {
        wait_gnome_proxy_settings_change(cancel, &ctx)
    } else if is_kde {
        wait_kde_proxy_settings_change(cancel, &ctx)
    } else {
        Err(anyhow!("不支持的桌面：{}", desktop))
    }
}

// ── GNOME ──────────────────────────────────────────────────────────────────

fn wait_gnome_proxy_settings_change(cancel: Arc<AtomicBool>, ctx: &LinuxExecContext) -> Result<()> {
    let schemas = [
        "org.gnome.system.proxy",
        "org.gnome.system.proxy.http",
        "org.gnome.system.proxy.https",
        "org.gnome.system.proxy.ftp",
        "org.gnome.system.proxy.socks",
    ];

    use std::sync::mpsc;
    let (changed_tx, changed_rx) = mpsc::channel::<()>();
    let (err_tx, err_rx) = mpsc::channel::<String>();

    let mut handles = vec![];
    for schema in &schemas {
        let schema = schema.to_string();
        let ctx_clone = ctx.clone();
        let cancel_clone = Arc::clone(&cancel);
        let changed_tx = changed_tx.clone();
        let err_tx = err_tx.clone();

        let handle = std::thread::spawn(move || {
            match wait_gsettings_schema_change(&cancel_clone, &ctx_clone, &schema) {
                Ok(changed) => {
                    if changed {
                        let _ = changed_tx.send(());
                    }
                }
                Err(e) => {
                    let _ = err_tx.send(e.to_string());
                }
            }
        });
        handles.push(handle);
    }
    drop(changed_tx);
    drop(err_tx);

    // Wait for first change, cancellation, or error
    loop {
        if cancel.load(Ordering::SeqCst) {
            return Err(anyhow!("cancelled"));
        }
        // Check for change
        if let Ok(()) = changed_rx.try_recv() {
            cancel.store(true, Ordering::SeqCst); // signal other threads
            for h in handles {
                let _ = h.join();
            }
            return Ok(());
        }
        // Check for error
        if let Ok(msg) = err_rx.try_recv() {
            return Err(anyhow!("{}", msg));
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// Returns Ok(true) if a change was detected, Ok(false) if cancelled.
fn wait_gsettings_schema_change(
    cancel: &Arc<AtomicBool>,
    ctx: &LinuxExecContext,
    schema: &str,
) -> Result<bool> {
    let mut child = ctx
        .command("gsettings", &["monitor", schema])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| anyhow!("启动 GNOME 代理设置监听失败：{}：{}", schema, e))?;

    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();

    loop {
        if cancel.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(false);
        }
        // Non-blocking line read attempt using peek
        let available = reader.fill_buf().map(|b| b.len()).unwrap_or(0);
        if available > 0 {
            line.clear();
            reader.read_line(&mut line)?;
            if !line.is_empty() {
                let _ = child.kill();
                let _ = child.wait();
                return Ok(true);
            }
        }
        // Check if process exited
        if let Ok(Some(_)) = child.try_wait() {
            if cancel.load(Ordering::SeqCst) {
                return Ok(false);
            }
            return Err(anyhow!("GNOME 代理设置监听已退出：{}", schema));
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

// ── KDE ────────────────────────────────────────────────────────────────────

fn wait_kde_proxy_settings_change(cancel: Arc<AtomicBool>, ctx: &LinuxExecContext) -> Result<()> {
    let config_path = kde_proxy_config_path(ctx)?;

    // If the config file doesn't exist, watch the directory
    let (watch_path, watch_dir) = if config_path.exists() {
        (config_path.clone(), false)
    } else {
        (
            config_path
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| config_path.clone()),
            true,
        )
    };

    use nix::sys::inotify::{AddWatchFlags, InitFlags, Inotify};
    let inotify = Inotify::init(InitFlags::IN_CLOEXEC)
        .map_err(|e| anyhow!("初始化 KDE 代理设置监听失败：{}", e))?;

    let flags = AddWatchFlags::IN_CLOSE_WRITE
        | AddWatchFlags::IN_MODIFY
        | AddWatchFlags::IN_MOVED_TO
        | AddWatchFlags::IN_CREATE
        | AddWatchFlags::IN_ATTRIB
        | AddWatchFlags::IN_DELETE_SELF
        | AddWatchFlags::IN_MOVE_SELF;

    inotify
        .add_watch(&watch_path, flags)
        .map_err(|e| anyhow!("监听 KDE 代理配置文件失败：{}：{}", watch_path.display(), e))?;

    loop {
        if cancel.load(Ordering::SeqCst) {
            return Err(anyhow!("cancelled"));
        }

        // Use poll with a 1-second timeout on the inotify fd
        use nix::poll::{PollFd, PollFlags, poll};
        use std::os::unix::io::AsFd;
        let mut fds = [PollFd::new(inotify.as_fd(), PollFlags::POLLIN)];

        match poll(&mut fds, 1000u16) {
            Err(nix::errno::Errno::EINTR) => continue,
            Err(e) => return Err(anyhow!("等待 KDE 代理配置文件变更失败：{}", e)),
            Ok(0) => continue, // timeout
            Ok(_) => {
                let events = inotify
                    .read_events()
                    .map_err(|e| anyhow!("读取 KDE 代理配置文件变更失败：{}", e))?;

                if !watch_dir {
                    return Ok(());
                }
                // Check if any event is for our target file
                let target = config_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");
                for event in &events {
                    if let Some(name) = &event.name {
                        if name.to_str().unwrap_or("") == target {
                            return Ok(());
                        }
                    }
                }
            }
        }
    }
}

fn kde_proxy_config_path(ctx: &LinuxExecContext) -> Result<PathBuf> {
    let config_home = ctx
        .env_map
        .get("XDG_CONFIG_HOME")
        .cloned()
        .unwrap_or_default();
    let config_home = if config_home.is_empty() {
        let home = ctx
            .env_map
            .get("HOME")
            .cloned()
            .or_else(|| dirs::home_dir().map(|p| p.to_string_lossy().into_owned()))
            .ok_or_else(|| anyhow!("无法获取用户配置目录"))?;
        PathBuf::from(home).join(".config")
    } else {
        PathBuf::from(config_home)
    };
    Ok(config_home.join("kioslaverc"))
}
