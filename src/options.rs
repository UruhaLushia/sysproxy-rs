/// 代理操作选项
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// 代理服务器地址（host:port）
    pub proxy: String,
    /// 绕过地址列表，逗号分隔
    pub bypass: String,
    /// PAC 脚本 URL
    pub pac_url: String,
    /// 指定网络设备/连接名称
    pub device: String,
    /// 仅对活跃的网络设备生效
    pub only_active_device: bool,
    /// 调用方进程 PID（Linux 用）
    pub peer_pid: i32,
    /// 调用方用户 UID（Linux 用）
    pub peer_uid: u32,
    /// 调用方用户 GID（Linux 用）
    pub peer_gid: u32,
    /// 环境变量列表（Linux 用）
    pub environment: Vec<String>,
    /// 是否并发执行；None 表示使用平台默认值
    pub concurrent: Option<bool>,
    /// Windows：使用注册表而非 Win32 API
    pub use_registry: bool,
}

/// 返回平台默认的并发设置（macOS 默认开启，其余关闭）
pub fn default_concurrent() -> bool {
    cfg!(target_os = "macos")
}

/// 根据 Options 解析实际的并发设置
pub fn resolve_concurrent(opt: Option<&Options>) -> bool {
    if let Some(opt) = opt {
        if let Some(c) = opt.concurrent {
            return c;
        }
    }
    default_concurrent()
}
