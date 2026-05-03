# @uruhalushia/sysproxy

Cross-platform system proxy settings for Node.js, backed by native Rust bindings.

The main package loads a platform-specific optional native package at runtime,
so package managers only need to install the binary package matching the current
OS, CPU, and libc.

## Install

```bash
npm install @uruhalushia/sysproxy
```

## Usage

CommonJS:

```js
const {
  queryProxySettings,
  setProxy,
  setPac,
  disableProxy,
  waitProxySettingsChange,
  ProxyGuard,
} = require('@uruhalushia/sysproxy')
```

ESM:

```js
import sysproxy from '@uruhalushia/sysproxy'

const {
  queryProxySettings,
  setProxy,
  setPac,
  disableProxy,
  waitProxySettingsChange,
  ProxyGuard,
} = sysproxy
```

Query current proxy settings:

```js
const { queryProxySettings } = require('@uruhalushia/sysproxy')

const config = queryProxySettings()
console.log(config.proxy.enable)

const servers = JSON.parse(config.proxy.servers)
console.log(servers.http_server)
```

Set HTTP/HTTPS/SOCKS proxy:

```js
const { setProxy } = require('@uruhalushia/sysproxy')

setProxy({
  proxy: '127.0.0.1:7890',
  bypass: 'localhost,127.0.0.0/8',
})
```

Set PAC proxy:

```js
const { setPac } = require('@uruhalushia/sysproxy')

setPac({ pacUrl: 'http://127.0.0.1:10000/pac' })
```

Disable proxy:

```js
const { disableProxy } = require('@uruhalushia/sysproxy')

disableProxy()
```

Wait for one proxy settings change:

```js
const { waitProxySettingsChange } = require('@uruhalushia/sysproxy')

waitProxySettingsChange()
console.log('proxy settings changed')
```

Guard proxy settings in the Rust layer:

```js
const { ProxyGuard } = require('@uruhalushia/sysproxy')

const guard = new ProxyGuard({
  proxy: '127.0.0.1:7890',
  bypass: 'localhost,127.0.0.0/8',
})

guard.onEvent((event) => {
  console.log(event.kind, event.message)
})

guard.start()

process.on('SIGINT', () => {
  guard.stop()
  process.exit(0)
})
```

Use PAC guard:

```js
const { ProxyGuard } = require('@uruhalushia/sysproxy')

const guard = new ProxyGuard({
  pacUrl: 'http://127.0.0.1:10000/pac',
})

guard.start()
```

## API

| Function | Description |
|----------|-------------|
| `queryProxySettings(options?)` | Return current proxy settings |
| `setProxy(options?)` | Set HTTP/HTTPS/SOCKS proxy |
| `setPac(options?)` | Set PAC proxy |
| `disableProxy(options?)` | Disable proxy |
| `waitProxySettingsChange(options?)` | Block until one system proxy settings change |
| `new ProxyGuard(options?)` | Create a Rust-layer proxy guard with `start()` / `stop()` |
| `ProxyGuard#onEvent(callback)` | Receive guard events: `changed`, `restored`, `restore_failed` |

## Options

| Field | Type | Description |
|-------|------|-------------|
| `proxy` | `string` | Proxy server, formatted as `host:port` |
| `bypass` | `string` | Proxy bypass list, comma-separated |
| `pacUrl` | `string` | PAC script URL |
| `device` | `string` | Target network device |
| `onlyActiveDevice` | `boolean` | Apply only to active network devices |
| `concurrent` | `boolean` | Enable concurrent apply where supported |
| `useRegistry` | `boolean` | Windows: use registry-backed settings |
