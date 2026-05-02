// prettier-ignore
/* eslint-disable */
// @ts-nocheck

const { existsSync } = require('fs')
const { join } = require('path')

const loadErrors = []

function isMusl() {
  try {
    return require('child_process').execSync('ldd --version', { encoding: 'utf8' }).includes('musl')
  } catch (_) {
    return false
  }
}

function requireLocal(tuple) {
  const filename = join(__dirname, `sysproxy.${tuple}.node`)
  if (!existsSync(filename)) {
    loadErrors.push(new Error(`Native binding not found: ${filename}`))
    return null
  }
  try {
    return require(filename)
  } catch (err) {
    loadErrors.push(err)
    return null
  }
}

function requireNative() {
  if (process.env.NAPI_RS_NATIVE_LIBRARY_PATH) {
    try {
      return require(process.env.NAPI_RS_NATIVE_LIBRARY_PATH)
    } catch (err) {
      loadErrors.push(err)
    }
  }

  if (process.platform === 'win32') {
    if (process.arch === 'x64') return requireLocal('win32-x64-msvc')
    if (process.arch === 'ia32') return requireLocal('win32-ia32-msvc')
    if (process.arch === 'arm64') return requireLocal('win32-arm64-msvc')
  } else if (process.platform === 'darwin') {
    if (process.arch === 'x64') return requireLocal('darwin-x64')
    if (process.arch === 'arm64') return requireLocal('darwin-arm64')
  } else if (process.platform === 'linux') {
    const musl = isMusl()
    if (process.arch === 'x64') return requireLocal(musl ? 'linux-x64-musl' : 'linux-x64-gnu')
    if (process.arch === 'arm64') return requireLocal(musl ? 'linux-arm64-musl' : 'linux-arm64-gnu')
    if (process.arch === 'riscv64' && !musl) return requireLocal('linux-riscv64-gnu')
  }

  loadErrors.push(new Error(`Unsupported OS or architecture: ${process.platform} ${process.arch}`))
  return null
}

const nativeBinding = requireNative()

if (!nativeBinding) {
  const error = new Error('Failed to load sysproxy native binding')
  error.cause = loadErrors
  throw error
}

module.exports = nativeBinding
module.exports.queryProxySettings = nativeBinding.queryProxySettings
module.exports.setProxy = nativeBinding.setProxy
module.exports.setPac = nativeBinding.setPac
module.exports.disableProxy = nativeBinding.disableProxy
module.exports.waitProxySettingsChange = nativeBinding.waitProxySettingsChange
module.exports.ProxyGuard = nativeBinding.ProxyGuard
