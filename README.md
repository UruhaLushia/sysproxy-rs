# sysproxy-rs

跨平台系统代理设置工具，支持读写操作系统级代理配置。Rust 实现，提供命令行工具和 Node.js 原生绑定。

[![Build](https://github.com/UruhaLushia/sysproxy-rs/actions/workflows/build.yml/badge.svg)](https://github.com/UruhaLushia/sysproxy-rs/actions/workflows/build.yml)
[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://www.gnu.org/licenses/gpl-3.0)

## 平台支持

| 平台 | 桌面环境 / 实现 |
|------|----------------|
| Linux | GNOME（gsettings）、KDE（kwriteconfig5/6） |
| macOS | networksetup |
| Windows | WinInet API / 注册表 |

## 命令行工具

### 安装

从 [Releases](https://github.com/UruhaLushia/sysproxy-rs/releases) 下载对应平台的二进制，或从源码编译：

```bash
cargo build --release
```

### 用法

```
sysproxy [选项] <子命令> [子命令选项]

选项：
  -a, --only-active-device      仅对活跃的网络设备生效
  -d, --device <设备>            指定网络设备
      --multithread              启用多线程并发设置
      --no-multithread           禁用多线程并发设置
      --registry                 Windows 使用注册表设置/查询代理

子命令：
  proxy  -s <服务器> [-b <绕过>]              设置系统代理
  pac    -u <PAC地址>                         设置 PAC 代理
  disable                                      取消代理设置
  status                                       查看当前代理设置（JSON 输出）
  watch                                        监听系统代理设置变更
  guard  -s <服务器> [-b <绕过>] [-u <PAC>]  守护系统代理设置
```

### 示例

```bash
# 设置 HTTP/HTTPS/SOCKS 代理
sysproxy proxy -s 127.0.0.1:7890 -b "localhost,127.0.0.0/8"

# 设置 PAC 代理
sysproxy pac -u http://127.0.0.1:10000/pac

# 查看当前代理设置
sysproxy status

# 取消代理
sysproxy disable

# 监听代理变更（Ctrl+C 退出）
sysproxy watch

# 守护代理设置（检测到被修改后自动恢复）
sysproxy guard -s 127.0.0.1:7890
```

## 作为 Rust 库使用

`sysproxy` 同时导出 Rust 库 API，可在其他 Rust 项目中直接查询、设置和监听系统代理。

### 引入依赖

```toml
[dependencies]
anyhow = "1"
sysproxy = { git = "https://github.com/UruhaLushia/sysproxy-rs.git" }
```

如果在同一个 workspace 或本地调试，也可以使用路径依赖：

```toml
[dependencies]
anyhow = "1"
sysproxy = { path = "../sysproxy-rs" }
```

### 基础用法

```rust
use sysproxy::{disable_proxy, query_proxy_settings, set_pac, set_proxy, Options};

fn main() -> anyhow::Result<()> {
    // 查询当前系统代理设置
    let cfg = query_proxy_settings(None)?;
    println!("proxy enabled: {}", cfg.proxy.enable);
    println!("pac enabled: {}", cfg.pac.enable);

    // 设置 HTTP/HTTPS/SOCKS 代理
    let opt = Options {
        proxy: "127.0.0.1:7890".to_string(),
        bypass: "localhost,127.0.0.0/8".to_string(),
        ..Default::default()
    };
    set_proxy(Some(&opt))?;

    // 设置 PAC 代理
    let pac_opt = Options {
        pac_url: "http://127.0.0.1:10000/pac".to_string(),
        ..Default::default()
    };
    set_pac(Some(&pac_opt))?;

    // 取消代理
    disable_proxy(None)?;

    Ok(())
}
```

### 监听代理变更

```rust
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use sysproxy::wait_proxy_settings_change;

fn main() -> anyhow::Result<()> {
    let cancel = Arc::new(AtomicBool::new(false));

    loop {
        match wait_proxy_settings_change(Arc::clone(&cancel), None) {
            Ok(()) => println!("proxy settings changed"),
            Err(e) if e.to_string().contains("cancelled") => break,
            Err(e) => return Err(e),
        }
    }

    Ok(())
}
```

### `Options` 字段

| 字段 | 说明 |
|------|------|
| `proxy` | 代理服务器地址，格式 `host:port` |
| `bypass` | 绕过地址，逗号分隔 |
| `pac_url` | PAC 脚本 URL |
| `device` | 指定网络设备 / 连接名称 |
| `only_active_device` | 仅对活跃网络设备生效 |
| `concurrent` | 是否并发执行；`None` 表示使用平台默认值 |
| `use_registry` | Windows 使用注册表设置 / 查询代理 |
| `peer_pid` / `peer_uid` / `peer_gid` / `environment` | Linux 调用方会话上下文；服务进程代用户设置代理时使用 |

## Node.js 绑定

`napi/` 目录提供通过 [napi-rs](https://napi.rs) 构建的原生 Node.js 绑定。

### 构建

```bash
cd napi
npm install
npm run build
```

### 使用

```js
const {
  queryProxySettings,
  setProxy,
  setPac,
  disableProxy,
} = require('./sysproxy.node')

// 查询当前代理设置
const cfg = queryProxySettings()
console.log(cfg.proxy.enable)              // true/false
const servers = JSON.parse(cfg.proxy.servers)
console.log(servers.http_server)           // "127.0.0.1:7890"

// 设置代理
setProxy({
  proxy: '127.0.0.1:7890',
  bypass: 'localhost,127.0.0.0/8',
})

// 设置 PAC
setPac({ pacUrl: 'http://127.0.0.1:10000/pac' })

// 取消代理
disableProxy()
```

### 可用函数

| 函数 | 说明 |
|------|------|
| `queryProxySettings(options?)` | 返回当前代理配置 |
| `setProxy(options?)` | 设置 HTTP/HTTPS/SOCKS 代理 |
| `setPac(options?)` | 设置 PAC 自动代理 |
| `disableProxy(options?)` | 取消代理设置 |

`options` 字段说明：

| 字段 | 类型 | 说明 |
|------|------|------|
| `proxy` | `string` | 代理地址，格式 `host:port` |
| `bypass` | `string` | 绕过地址，逗号分隔 |
| `pacUrl` | `string` | PAC 脚本 URL |
| `device` | `string` | 指定网络设备（macOS） |
| `onlyActiveDevice` | `boolean` | 仅对活跃设备生效 |
| `concurrent` | `boolean` | 并发设置（macOS 默认 true） |
| `useRegistry` | `boolean` | Windows 使用注册表 |

## 从源码编译

依赖：Rust 1.85+（edition 2024）

```bash
git clone https://github.com/UruhaLushia/sysproxy-rs.git
cd sysproxy-rs
cargo build --release
# 二进制输出到 target/release/sysproxy
```

## 许可证

本项目基于 [GNU General Public License v3.0](LICENSE) 发布。
