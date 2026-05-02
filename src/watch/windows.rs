use anyhow::{anyhow, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::options::Options;

/// 监听 Windows 注册表代理设置变更
pub fn wait_proxy_settings_change(
    cancel: Arc<AtomicBool>,
    opt: Option<&Options>,
) -> Result<()> {
    validate_registry_target(opt)?;

    use windows::Win32::Foundation::{ERROR_SUCCESS, HANDLE};
    use windows::Win32::System::Registry::{
        RegNotifyChangeKeyValue, HKEY, REG_NOTIFY_CHANGE_LAST_SET, REG_NOTIFY_CHANGE_NAME,
        REG_NOTIFY_THREAD_AGNOSTIC,
    };
    use windows::Win32::System::Threading::{CreateEventW, SetEvent, WaitForMultipleObjects, INFINITE};
    use windows::core::PCWSTR;

    let paths = [
        r"Software\Microsoft\Windows\CurrentVersion\Internet Settings",
        r"Software\Microsoft\Windows\CurrentVersion\Internet Settings\Connections",
    ];

    let mut keys: Vec<HKEY> = Vec::new();
    let mut events: Vec<HANDLE> = Vec::new();

    // Open registry keys
    for path in &paths {
        let key = open_current_user_key_notify(path)?;
        keys.push(key);
    }

    // Create events for each key
    for key in &keys {
        let event = unsafe { CreateEventW(None, false, false, PCWSTR::null()) }
            .map_err(|e| anyhow!("创建代理设置变更事件失败：{}", e))?;
        events.push(event);

        let filter = REG_NOTIFY_CHANGE_LAST_SET | REG_NOTIFY_CHANGE_NAME | REG_NOTIFY_THREAD_AGNOSTIC;
        let result = unsafe {
            RegNotifyChangeKeyValue(
                *key,
                false,
                filter,
                Some(event),
                true,
            )
        };
        if result != ERROR_SUCCESS {
            // Retry without REG_NOTIFY_THREAD_AGNOSTIC
            let filter2 = REG_NOTIFY_CHANGE_LAST_SET | REG_NOTIFY_CHANGE_NAME;
            let result2 = unsafe {
                RegNotifyChangeKeyValue(*key, false, filter2, Some(event), true)
            };
            if result2 != ERROR_SUCCESS {
                cleanup_keys_events(&keys, &events);
                return Err(anyhow!("监听代理设置注册表失败：{}", result2.0));
            }
        }
    }

    // Create cancel event
    let cancel_event = unsafe { CreateEventW(None, false, false, PCWSTR::null()) }
        .map_err(|e| anyhow!("创建取消事件失败：{}", e))?;
    events.push(cancel_event);

    let cancel_clone = Arc::clone(&cancel);
    let cancel_handle_usize = cancel_event.0 as usize;
    std::thread::spawn(move || {
        loop {
            if cancel_clone.load(Ordering::SeqCst) {
                let h = windows::Win32::Foundation::HANDLE(cancel_handle_usize as *mut _);
                unsafe { let _ = SetEvent(h); };
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    });

    let handles: Vec<HANDLE> = events.clone();
    let result = unsafe {
        WaitForMultipleObjects(&handles, false, INFINITE)
    };

    cleanup_keys_events(&keys, &events);

    let idx = result.0;
    let cancel_idx = (handles.len() - 1) as u32;

    if idx == cancel_idx {
        return Err(anyhow!("cancelled"));
    }
    if idx >= handles.len() as u32 {
        return Err(anyhow!("等待代理设置变更失败：返回值 {}", idx));
    }
    Ok(())
}

fn validate_registry_target(opt: Option<&Options>) -> Result<()> {
    if opt
        .map(|o| o.use_registry && !o.device.is_empty())
        .unwrap_or(false)
    {
        return Err(anyhow!("注册表模式不支持指定网络设备"));
    }
    Ok(())
}

fn open_current_user_key_notify(path: &str) -> Result<windows::Win32::System::Registry::HKEY> {
    use windows::Win32::System::Registry::{
        RegOpenCurrentUser, RegOpenKeyExW, HKEY, KEY_NOTIFY, REG_SAM_FLAGS,
    };
    use windows::core::PCWSTR;

    let path_wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let mut current_user = HKEY::default();
        let r = RegOpenCurrentUser(KEY_NOTIFY.0, &mut current_user);
        if r != windows::Win32::Foundation::ERROR_SUCCESS {
            return Err(anyhow!("RegOpenCurrentUser 失败：{}", r.0));
        }
        let mut key = HKEY::default();
        let r = RegOpenKeyExW(
            current_user,
            PCWSTR(path_wide.as_ptr()),
            None,
            REG_SAM_FLAGS(KEY_NOTIFY.0),
            &mut key,
        );
        let _ = windows::Win32::System::Registry::RegCloseKey(current_user);
        if r != windows::Win32::Foundation::ERROR_SUCCESS {
            return Err(anyhow!("RegOpenKeyExW({}) 失败：{}", path, r.0));
        }
        Ok(key)
    }
}

fn cleanup_keys_events(
    keys: &[windows::Win32::System::Registry::HKEY],
    events: &[windows::Win32::Foundation::HANDLE],
) {
    for &key in keys {
        unsafe { let _ = windows::Win32::System::Registry::RegCloseKey(key); };
    }
    for &ev in events {
        unsafe { let _ = windows::Win32::Foundation::CloseHandle(ev); };
    }
}
