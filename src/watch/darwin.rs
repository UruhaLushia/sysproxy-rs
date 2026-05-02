use anyhow::{anyhow, Result};
use std::os::fd::AsRawFd;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::options::Options;

const SYSTEM_CONFIGURATION_PREFERENCES_PATH: &str =
    "/Library/Preferences/SystemConfiguration/preferences.plist";

pub fn wait_proxy_settings_change(
    cancel: Arc<AtomicBool>,
    _opt: Option<&Options>,
) -> Result<()> {
    use nix::sys::event::{EvFlags, EventFilter, FilterFlag, KEvent, Kqueue};
    use nix::sys::stat::stat;

    let o_evtonly = nix::fcntl::OFlag::from_bits_truncate(0x8000);
    let fd = nix::fcntl::open(
        SYSTEM_CONFIGURATION_PREFERENCES_PATH,
        o_evtonly,
        nix::sys::stat::Mode::empty(),
    )
    .map_err(|e| anyhow!("打开 macOS 系统代理配置监听文件失败：{}", e))?;

    let kq = Kqueue::new().map_err(|e| anyhow!("初始化 macOS 系统代理配置监听失败：{}", e))?;

    let change = KEvent::new(
        fd.as_raw_fd() as usize,
        EventFilter::EVFILT_VNODE,
        EvFlags::EV_ADD | EvFlags::EV_ENABLE | EvFlags::EV_CLEAR,
        FilterFlag::NOTE_WRITE
            | FilterFlag::NOTE_EXTEND
            | FilterFlag::NOTE_ATTRIB
            | FilterFlag::NOTE_RENAME
            | FilterFlag::NOTE_DELETE
            | FilterFlag::NOTE_REVOKE,
        0,
        0,
    );

    kq.kevent(&[change], &mut [], None)
        .map_err(|e| anyhow!("注册 macOS 系统代理配置监听失败：{}", e))?;

    let timeout = nix::libc::timespec { tv_sec: 1, tv_nsec: 0 };
    let mut events = vec![KEvent::new(
        0,
        EventFilter::EVFILT_VNODE,
        EvFlags::empty(),
        FilterFlag::empty(),
        0,
        0,
    )];

    loop {
        if cancel.load(Ordering::SeqCst) {
            return Err(anyhow!("cancelled"));
        }

        match kq.kevent(&[], &mut events, Some(timeout)) {
            Err(nix::errno::Errno::EINTR) => continue,
            Err(e) => {
                return Err(anyhow!("等待 macOS 系统代理配置变更失败：{}", e));
            }
            Ok(0) => {
                if stat(SYSTEM_CONFIGURATION_PREFERENCES_PATH).is_err() {
                    return Err(anyhow!("读取 macOS 系统代理配置文件失败"));
                }
            }
            Ok(_) => {
                return Ok(());
            }
        }
    }
}
