pub mod options;
pub mod platform;
pub mod types;
pub mod watch;

pub use options::{default_concurrent, resolve_concurrent, Options};
pub use types::{clean_output, format_server, parse_server_string, PacInfo, ProxyConfig, ProxyInfo, ServerAddr};

pub use platform::{disable_proxy, query_proxy_settings, set_pac, set_proxy};
pub use watch::wait_proxy_settings_change;
