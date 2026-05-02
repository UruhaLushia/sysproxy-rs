use anyhow::{anyhow, Result};
use std::collections::HashMap;

use crate::options::Options;
use crate::types::ProxyConfig;

use windows::core::PCWSTR;
use windows::Win32::Networking::WinInet::{
    InternetQueryOptionW, InternetSetOptionW, INTERNET_OPTION_PER_CONNECTION_OPTION,
    INTERNET_OPTION_PROXY_SETTINGS_CHANGED, INTERNET_OPTION_REFRESH,
    INTERNET_PER_CONN_AUTOCONFIG_URL, INTERNET_PER_CONN_FLAGS, INTERNET_PER_CONN_OPTIONW,
    INTERNET_PER_CONN_OPTION_LISTW, INTERNET_PER_CONN_PROXY_BYPASS,
    INTERNET_PER_CONN_PROXY_SERVER, PROXY_TYPE_AUTO_PROXY_URL, PROXY_TYPE_DIRECT,
    PROXY_TYPE_PROXY,
};
use windows::Win32::System::Registry::*;

const INTERNET_SETTINGS_REG_PATH: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";

pub fn disable_proxy(opt: Option<&Options>) -> Result<()> {
    if use_registry_settings(opt) {
        return disable_proxy_registry(opt);
    }

    let options = vec![INTERNET_PER_CONN_OPTIONW {
        dwOption: INTERNET_PER_CONN_FLAGS,
        Value: windows::Win32::Networking::WinInet::INTERNET_PER_CONN_OPTIONW_0 {
            dwValue: PROXY_TYPE_DIRECT,
        },
    }];
    refresh_and_apply_settings(&options, opt)
}

pub fn set_proxy(opt: Option<&Options>) -> Result<()> {
    if use_registry_settings(opt) {
        return set_proxy_registry(opt);
    }

    let mut proxy = opt.map(|o| o.proxy.clone()).unwrap_or_default();
    let mut bypass = opt.map(|o| o.bypass.clone()).unwrap_or_default();

    if proxy.is_empty() || bypass.is_empty() {
        let config = query_proxy_settings(None)?;
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

    let proxy_wide = to_wide_null(&proxy);
    let bypass_wide = to_wide_null(&bypass);

    let options = vec![
        INTERNET_PER_CONN_OPTIONW {
            dwOption: INTERNET_PER_CONN_FLAGS,
            Value: windows::Win32::Networking::WinInet::INTERNET_PER_CONN_OPTIONW_0 {
                dwValue: PROXY_TYPE_PROXY,
            },
        },
        INTERNET_PER_CONN_OPTIONW {
            dwOption: INTERNET_PER_CONN_PROXY_SERVER,
            Value: windows::Win32::Networking::WinInet::INTERNET_PER_CONN_OPTIONW_0 {
                pszValue: windows::core::PWSTR(proxy_wide.as_ptr() as *mut u16),
            },
        },
        INTERNET_PER_CONN_OPTIONW {
            dwOption: INTERNET_PER_CONN_PROXY_BYPASS,
            Value: windows::Win32::Networking::WinInet::INTERNET_PER_CONN_OPTIONW_0 {
                pszValue: windows::core::PWSTR(bypass_wide.as_ptr() as *mut u16),
            },
        },
    ];
    let result = refresh_and_apply_settings(&options, opt);
    drop(proxy_wide);
    drop(bypass_wide);
    result
}

pub fn set_pac(opt: Option<&Options>) -> Result<()> {
    if use_registry_settings(opt) {
        return set_pac_registry(opt);
    }

    let pac_url = opt.map(|o| o.pac_url.clone()).unwrap_or_default();
    if pac_url.is_empty() {
        let options = vec![INTERNET_PER_CONN_OPTIONW {
            dwOption: INTERNET_PER_CONN_FLAGS,
            Value: windows::Win32::Networking::WinInet::INTERNET_PER_CONN_OPTIONW_0 {
                dwValue: PROXY_TYPE_AUTO_PROXY_URL,
            },
        }];
        return refresh_and_apply_settings(&options, opt);
    }

    let pac_wide = to_wide_null(&pac_url);
    let options = vec![
        INTERNET_PER_CONN_OPTIONW {
            dwOption: INTERNET_PER_CONN_FLAGS,
            Value: windows::Win32::Networking::WinInet::INTERNET_PER_CONN_OPTIONW_0 {
                dwValue: PROXY_TYPE_AUTO_PROXY_URL,
            },
        },
        INTERNET_PER_CONN_OPTIONW {
            dwOption: INTERNET_PER_CONN_AUTOCONFIG_URL,
            Value: windows::Win32::Networking::WinInet::INTERNET_PER_CONN_OPTIONW_0 {
                pszValue: windows::core::PWSTR(pac_wide.as_ptr() as *mut u16),
            },
        },
    ];
    let result = refresh_and_apply_settings(&options, opt);
    drop(pac_wide);
    result
}

pub fn query_proxy_settings(opt: Option<&Options>) -> Result<ProxyConfig> {
    if use_registry_settings(opt) {
        validate_registry_target(opt)?;
        return query_proxy_settings_registry();
    }

    let mut options = [
        INTERNET_PER_CONN_OPTIONW {
            dwOption: INTERNET_PER_CONN_FLAGS,
            Value: Default::default(),
        },
        INTERNET_PER_CONN_OPTIONW {
            dwOption: INTERNET_PER_CONN_PROXY_SERVER,
            Value: Default::default(),
        },
        INTERNET_PER_CONN_OPTIONW {
            dwOption: INTERNET_PER_CONN_PROXY_BYPASS,
            Value: Default::default(),
        },
        INTERNET_PER_CONN_OPTIONW {
            dwOption: INTERNET_PER_CONN_AUTOCONFIG_URL,
            Value: Default::default(),
        },
    ];

    let mut list_size = std::mem::size_of::<INTERNET_PER_CONN_OPTION_LISTW>() as u32;
    let mut list = INTERNET_PER_CONN_OPTION_LISTW {
        dwSize: list_size,
        pszConnection: windows::core::PWSTR::null(),
        dwOptionCount: 4,
        dwOptionError: 0,
        pOptions: options.as_mut_ptr(),
    };

    unsafe {
        InternetQueryOptionW(
            None,
            INTERNET_OPTION_PER_CONNECTION_OPTION,
            Some(&mut list as *mut _ as *mut std::ffi::c_void),
            &mut list_size,
        )
        .map_err(|e| anyhow!("查询失败：{}", e))?;
    }

    let flags = unsafe { options[0].Value.dwValue };
    let mut config = ProxyConfig::default();
    config.proxy.enable = (flags & PROXY_TYPE_PROXY) != 0;
    config.proxy.servers = HashMap::from([(
        "http_server".into(),
        unsafe { pwstr_to_string(options[1].Value.pszValue) },
    )]);
    config.proxy.bypass = unsafe { pwstr_to_string(options[2].Value.pszValue) };
    config.pac.enable = (flags & PROXY_TYPE_AUTO_PROXY_URL) != 0;
    config.pac.url = unsafe { pwstr_to_string(options[3].Value.pszValue) };

    Ok(config)
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn use_registry_settings(opt: Option<&Options>) -> bool {
    opt.map(|o| o.use_registry).unwrap_or(false)
}

fn validate_registry_target(opt: Option<&Options>) -> Result<()> {
    if opt.map(|o| !o.device.is_empty()).unwrap_or(false) {
        return Err(anyhow!("注册表模式不支持指定网络设备"));
    }
    Ok(())
}

fn get_target_connections(opt: Option<&Options>) -> Result<Vec<String>> {
    if let Some(opt) = opt {
        if !opt.device.is_empty() {
            return Ok(vec![opt.device.clone()]);
        }
        if opt.only_active_device {
            return Ok(vec![String::new()]);
        }
    }
    let mut names = enum_all_connection_names()?;
    names.push(String::new()); // default connection
    Ok(names)
}

fn refresh_and_apply_settings(
    options: &[INTERNET_PER_CONN_OPTIONW],
    opt: Option<&Options>,
) -> Result<()> {
    let connections = get_target_connections(opt)?;
    let concurrent = crate::options::resolve_concurrent(opt);

    let opts_ptr = options.as_ptr() as usize;
    let opts_len = options.len();
    let apply_conn = move |name: &str| -> Result<()> {
        let mut local_options: Vec<INTERNET_PER_CONN_OPTIONW> =
            unsafe { std::slice::from_raw_parts(opts_ptr as *const INTERNET_PER_CONN_OPTIONW, opts_len) }.to_vec();
        let mut name_wide: Vec<u16>;
        let psz_conn = if !name.is_empty() {
            name_wide = to_wide_null(name);
            windows::core::PWSTR(name_wide.as_mut_ptr())
        } else {
            windows::core::PWSTR::null()
        };

        let list = INTERNET_PER_CONN_OPTION_LISTW {
            dwSize: std::mem::size_of::<INTERNET_PER_CONN_OPTION_LISTW>() as u32,
            pszConnection: psz_conn,
            dwOptionCount: local_options.len() as u32,
            dwOptionError: 0,
            pOptions: local_options.as_mut_ptr(),
        };

        unsafe {
            InternetSetOptionW(
                None,
                INTERNET_OPTION_PER_CONNECTION_OPTION,
                Some(&list as *const _ as *const std::ffi::c_void),
                std::mem::size_of::<INTERNET_PER_CONN_OPTION_LISTW>() as u32,
            )
            .map_err(|e| anyhow!("设置 {} 连接失败：{}", name, e))?;
        }
        Ok(())
    };

    if concurrent {
        apply_concurrent(&connections, apply_conn)?;
    } else {
        for name in &connections {
            apply_conn(name)?;
        }
    }

    unsafe {
        let _ = InternetSetOptionW(
            None,
            INTERNET_OPTION_PROXY_SETTINGS_CHANGED,
            None,
            0,
        );
        let _ = InternetSetOptionW(None, INTERNET_OPTION_REFRESH, None, 0);
    }
    Ok(())
}

fn apply_concurrent<F>(names: &[String], f: F) -> Result<()>
where
    F: Fn(&str) -> Result<()> + Send + Sync + 'static,
{
    use std::sync::{Arc, Mutex};
    let f = Arc::new(f);
    let error: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let mut handles = vec![];

    for name in names {
        let name = name.clone();
        let f = Arc::clone(&f);
        let error = Arc::clone(&error);
        let handle = std::thread::spawn(move || {
            if let Err(e) = f(&name) {
                let mut lock = error.lock().unwrap();
                if lock.is_none() {
                    *lock = Some(e.to_string());
                }
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

fn enum_all_connection_names() -> Result<Vec<String>> {
    let key = open_current_user_key(
        r"Software\Microsoft\Windows\CurrentVersion\Internet Settings\Connections",
        REG_SAM_FLAGS(KEY_READ.0),
    )?;

    let reserved = ["DefaultConnectionSettings", "SavedLegacySettings"];
    let mut names = vec![];

    let mut idx = 0u32;
    loop {
        let mut name_buf = [0u16; 256];
        let mut name_len = name_buf.len() as u32;
        let result = unsafe {
            RegEnumValueW(
                key,
                idx,
                Some(windows::core::PWSTR(name_buf.as_mut_ptr())),
                &mut name_len,
                None,
                None,
                None,
                None,
            )
        };
        if result != windows::Win32::Foundation::ERROR_SUCCESS {
            break;
        }
        let name = String::from_utf16_lossy(&name_buf[..name_len as usize]);
        if !name.is_empty() && !reserved.contains(&name.as_str()) {
            names.push(name);
        }
        idx += 1;
    }
    unsafe { let _ = RegCloseKey(key); };
    Ok(names)
}

// ── Registry helpers ────────────────────────────────────────────────────────

fn open_current_user_key(path: &str, access: REG_SAM_FLAGS) -> Result<HKEY> {
    unsafe {
        let mut current_user = HKEY::default();
        let result = RegOpenCurrentUser(access.0, &mut current_user);
        if result != windows::Win32::Foundation::ERROR_SUCCESS {
            return Err(anyhow!("RegOpenCurrentUser 失败：{}", result.0));
        }

        let path_wide = to_wide_null(path);
        let mut key = HKEY::default();
        let result = RegOpenKeyExW(
            current_user,
            PCWSTR(path_wide.as_ptr()),
            None,
            access,
            &mut key,
        );
        let _ = RegCloseKey(current_user);
        if result != windows::Win32::Foundation::ERROR_SUCCESS {
            return Err(anyhow!("RegOpenKeyExW({}) 失败：{}", path, result.0));
        }
        Ok(key)
    }
}

fn open_current_user_key_write(path: &str) -> Result<HKEY> {
    open_current_user_key(path, REG_SAM_FLAGS(KEY_SET_VALUE.0))
}

fn open_current_user_key_read(path: &str) -> Result<HKEY> {
    open_current_user_key(path, REG_SAM_FLAGS(KEY_READ.0))
}

fn reg_set_dword(key: HKEY, name: &str, value: u32) -> Result<()> {
    let name_wide = to_wide_null(name);
    let data = value.to_le_bytes();
    let result = unsafe {
        RegSetValueExW(
            key,
            PCWSTR(name_wide.as_ptr()),
            None,
            REG_DWORD,
            Some(&data),
        )
    };
    if result != windows::Win32::Foundation::ERROR_SUCCESS {
        return Err(anyhow!("RegSetValueExW({}) 失败", name));
    }
    Ok(())
}

fn reg_set_string(key: HKEY, name: &str, value: &str) -> Result<()> {
    let name_wide = to_wide_null(name);
    let value_wide = to_wide_null(value);
    let data: &[u8] = unsafe {
        std::slice::from_raw_parts(
            value_wide.as_ptr() as *const u8,
            value_wide.len() * 2,
        )
    };
    let result = unsafe {
        RegSetValueExW(
            key,
            PCWSTR(name_wide.as_ptr()),
            None,
            REG_SZ,
            Some(data),
        )
    };
    if result != windows::Win32::Foundation::ERROR_SUCCESS {
        return Err(anyhow!("RegSetValueExW({}) 失败", name));
    }
    Ok(())
}

fn reg_delete_value(key: HKEY, name: &str) -> Result<()> {
    let name_wide = to_wide_null(name);
    let result = unsafe { RegDeleteValueW(key, PCWSTR(name_wide.as_ptr())) };
    if result == windows::Win32::Foundation::ERROR_FILE_NOT_FOUND {
        return Ok(());
    }
    if result != windows::Win32::Foundation::ERROR_SUCCESS {
        return Err(anyhow!("RegDeleteValueW({}) 失败", name));
    }
    Ok(())
}

fn reg_read_dword(key: HKEY, name: &str) -> Result<u32> {
    let name_wide = to_wide_null(name);
    let mut data = [0u8; 4];
    let mut size = data.len() as u32;
    let mut reg_type = REG_VALUE_TYPE::default();
    let result = unsafe {
        RegQueryValueExW(
            key,
            PCWSTR(name_wide.as_ptr()),
            None,
            Some(&mut reg_type),
            Some(data.as_mut_ptr()),
            Some(&mut size),
        )
    };
    if result == windows::Win32::Foundation::ERROR_FILE_NOT_FOUND {
        return Ok(0);
    }
    if result != windows::Win32::Foundation::ERROR_SUCCESS {
        return Err(anyhow!("RegQueryValueExW({}) 失败", name));
    }
    Ok(u32::from_le_bytes(data))
}

fn reg_read_string(key: HKEY, name: &str) -> Result<String> {
    let name_wide = to_wide_null(name);
    let mut size = 0u32;
    let result = unsafe {
        RegQueryValueExW(
            key,
            PCWSTR(name_wide.as_ptr()),
            None,
            None,
            None,
            Some(&mut size),
        )
    };
    if result == windows::Win32::Foundation::ERROR_FILE_NOT_FOUND {
        return Ok(String::new());
    }
    if result != windows::Win32::Foundation::ERROR_SUCCESS {
        return Err(anyhow!("RegQueryValueExW({}) 失败（获取大小）", name));
    }
    let mut buf = vec![0u8; size as usize];
    let result = unsafe {
        RegQueryValueExW(
            key,
            PCWSTR(name_wide.as_ptr()),
            None,
            None,
            Some(buf.as_mut_ptr()),
            Some(&mut size),
        )
    };
    if result != windows::Win32::Foundation::ERROR_SUCCESS {
        return Err(anyhow!("RegQueryValueExW({}) 失败（读取值）", name));
    }
    // buf contains UTF-16LE bytes
    let wchars: Vec<u16> = buf
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    let s = String::from_utf16_lossy(&wchars);
    Ok(s.trim_end_matches('\0').to_string())
}

// ── Registry proxy operations ────────────────────────────────────────────

fn disable_proxy_registry(opt: Option<&Options>) -> Result<()> {
    validate_registry_target(opt)?;
    let key = open_current_user_key_write(INTERNET_SETTINGS_REG_PATH)?;
    reg_set_dword(key, "ProxyEnable", 0)?;
    reg_set_dword(key, "AutoDetect", 0)?;
    reg_delete_value(key, "AutoConfigURL")?;
    unsafe { let _ = RegCloseKey(key); };
    Ok(())
}

fn set_proxy_registry(opt: Option<&Options>) -> Result<()> {
    validate_registry_target(opt)?;
    let mut proxy = opt.map(|o| o.proxy.clone()).unwrap_or_default();
    let mut bypass = opt.map(|o| o.bypass.clone()).unwrap_or_default();

    if proxy.is_empty() || bypass.is_empty() {
        let config = query_proxy_settings_registry()?;
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

    let key = open_current_user_key_write(INTERNET_SETTINGS_REG_PATH)?;
    reg_set_dword(key, "ProxyEnable", 1)?;
    reg_set_string(key, "ProxyServer", &proxy)?;
    reg_set_string(key, "ProxyOverride", &bypass)?;
    reg_set_dword(key, "AutoDetect", 0)?;
    reg_delete_value(key, "AutoConfigURL")?;
    unsafe { let _ = RegCloseKey(key); };
    Ok(())
}

fn set_pac_registry(opt: Option<&Options>) -> Result<()> {
    validate_registry_target(opt)?;
    let mut pac_url = opt.map(|o| o.pac_url.clone()).unwrap_or_default();

    if pac_url.is_empty() {
        let config = query_proxy_settings_registry()?;
        pac_url = config.pac.url.clone();
    }

    let key = open_current_user_key_write(INTERNET_SETTINGS_REG_PATH)?;
    reg_set_dword(key, "ProxyEnable", 0)?;
    reg_set_dword(key, "AutoDetect", 0)?;
    if !pac_url.is_empty() {
        reg_set_string(key, "AutoConfigURL", &pac_url)?;
    }
    unsafe { let _ = RegCloseKey(key); };
    Ok(())
}

fn query_proxy_settings_registry() -> Result<ProxyConfig> {
    let key = open_current_user_key_read(INTERNET_SETTINGS_REG_PATH)?;
    let proxy_enable = reg_read_dword(key, "ProxyEnable")?;
    let proxy_server = reg_read_string(key, "ProxyServer")?;
    let proxy_override = reg_read_string(key, "ProxyOverride")?;
    let auto_config_url = reg_read_string(key, "AutoConfigURL")?;
    unsafe { let _ = RegCloseKey(key); };

    let mut config = ProxyConfig::default();
    config.proxy.enable = proxy_enable != 0;
    config.proxy.servers = HashMap::from([("http_server".into(), proxy_server)]);
    config.proxy.bypass = proxy_override;
    config.pac.enable = !auto_config_url.is_empty();
    config.pac.url = auto_config_url;
    Ok(config)
}

// ── Utility ─────────────────────────────────────────────────────────────────

fn to_wide_null(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe fn pwstr_to_string(p: windows::core::PWSTR) -> String {
    if p.is_null() {
        return String::new();
    }
    unsafe { p.to_string() }.unwrap_or_default()
}
