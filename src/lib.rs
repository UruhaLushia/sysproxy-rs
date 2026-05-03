pub mod guard;
pub mod options;
pub mod platform;
pub mod types;
pub mod watch;

pub use guard::{
    GuardEvent, apply_guard_proxy_settings, guard_proxy_settings, guard_proxy_settings_after_apply,
    guard_proxy_settings_after_apply_with_events,
};
pub use options::{Options, default_concurrent, resolve_concurrent};
pub use types::{
    PacInfo, ProxyConfig, ProxyInfo, ServerAddr, clean_output, format_server, parse_server_string,
};

pub use platform::{disable_proxy, query_proxy_settings, set_pac, set_proxy};
pub use watch::wait_proxy_settings_change;
