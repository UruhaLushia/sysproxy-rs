use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

use crate::options::Options;

/// Linux 子进程执行上下文（处理用户身份和环境变量）
#[derive(Debug, Clone)]
pub struct LinuxExecContext {
    pub env_map: HashMap<String, String>,
    pub use_credential: bool,
    pub uid: u32,
    pub gid: u32,
}

impl LinuxExecContext {
    pub fn new(opt: Option<&Options>) -> Result<Self> {
        let uid = nix::unistd::getuid().as_raw();
        let gid = nix::unistd::getgid().as_raw();

        let mut ctx = LinuxExecContext {
            env_map: current_process_env(),
            use_credential: false,
            uid,
            gid,
        };

        if let Some(opt) = opt {
            if !opt.environment.is_empty() {
                ctx.env_map =
                    merge_env_maps(session_base_env(), env_slice_to_map(&opt.environment));
                if opt.peer_uid != 0 || opt.peer_gid != 0 {
                    ctx.uid = opt.peer_uid;
                    ctx.gid = opt.peer_gid;
                    ctx.use_credential = nix::unistd::geteuid().is_root();
                }
                ensure_linux_session_env(&mut ctx.env_map, ctx.uid);
            } else if opt.peer_pid > 0 {
                let peer_env = read_process_env(opt.peer_pid)
                    .with_context(|| format!("读取连接进程环境失败：pid={}", opt.peer_pid))?;
                ctx.env_map = merge_env_maps(session_base_env(), peer_env);
                ctx.uid = opt.peer_uid;
                ctx.gid = opt.peer_gid;
                ctx.use_credential = nix::unistd::geteuid().is_root();
                ensure_linux_session_env(&mut ctx.env_map, ctx.uid);
            }
        }

        Ok(ctx)
    }

    /// 构建以当前用户身份执行的命令
    pub fn command(&self, program: &str, args: &[&str]) -> Command {
        let mut cmd = Command::new(program);
        cmd.args(args);
        cmd.env_clear();
        for (k, v) in &self.env_map {
            cmd.env(k, v);
        }
        if self.use_credential {
            use std::os::unix::process::CommandExt;
            let uid = self.uid;
            let gid = self.gid;
            unsafe {
                cmd.pre_exec(move || {
                    nix::unistd::setgid(nix::unistd::Gid::from_raw(gid)).map_err(|e| {
                        std::io::Error::from_raw_os_error(e as i32)
                    })?;
                    nix::unistd::setuid(nix::unistd::Uid::from_raw(uid)).map_err(|e| {
                        std::io::Error::from_raw_os_error(e as i32)
                    })?;
                    Ok(())
                });
            }
        }
        cmd
    }
}

fn current_process_env() -> HashMap<String, String> {
    std::env::vars().collect()
}

fn session_base_env() -> HashMap<String, String> {
    let mut env = HashMap::new();
    for key in &["PATH", "LANG", "LC_ALL", "LC_CTYPE", "LC_MESSAGES", "TERM"] {
        if let Ok(val) = std::env::var(key) {
            if !val.is_empty() {
                env.insert(key.to_string(), val);
            }
        }
    }
    env
}

fn merge_env_maps(
    base: HashMap<String, String>,
    override_map: HashMap<String, String>,
) -> HashMap<String, String> {
    let mut merged = base;
    merged.extend(override_map);
    merged
}

pub fn env_slice_to_map(env: &[String]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for item in env {
        if let Some(idx) = item.find('=') {
            map.insert(item[..idx].to_string(), item[idx + 1..].to_string());
        }
    }
    map
}

fn ensure_linux_session_env(env_map: &mut HashMap<String, String>, uid: u32) {
    env_map
        .entry("XDG_RUNTIME_DIR".to_string())
        .or_insert_with(|| format!("/run/user/{}", uid));
    if !env_map.contains_key("DBUS_SESSION_BUS_ADDRESS") {
        if let Some(xdg) = env_map.get("XDG_RUNTIME_DIR").cloned() {
            env_map.insert(
                "DBUS_SESSION_BUS_ADDRESS".to_string(),
                format!("unix:path={}/bus", xdg),
            );
        }
    }
}

fn read_process_env(pid: i32) -> Result<HashMap<String, String>> {
    let path = PathBuf::from(format!("/proc/{}/environ", pid));
    let data =
        std::fs::read(&path).with_context(|| format!("无法读取 {}", path.display()))?;

    let mut map = HashMap::new();
    for item in data.split(|&b| b == 0) {
        if item.is_empty() {
            continue;
        }
        let s = String::from_utf8_lossy(item);
        if let Some(idx) = s.find('=') {
            map.insert(s[..idx].to_string(), s[idx + 1..].to_string());
        }
    }
    Ok(map)
}
