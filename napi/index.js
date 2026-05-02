// prettier-ignore
/* eslint-disable */
// @ts-nocheck

import { existsSync } from 'fs'
import { join } from 'path'
import { createRequire } from 'module'
import packageJson from './package.json' with { type: 'json' }

const require = createRequire(import.meta.url)
const __dirname = new URL('.', import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1')

const packageName = '@uruhalushia/sysproxy'
const packageVersion = packageJson.version
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

function requirePackage(tuple) {
  const nativePackage = `${packageName}-${tuple}`
  try {
    const binding = require(nativePackage)
    const bindingPackageVersion = require(`${nativePackage}/package.json`).version
    if (
      bindingPackageVersion !== packageVersion &&
      process.env.NAPI_RS_ENFORCE_VERSION_CHECK &&
      process.env.NAPI_RS_ENFORCE_VERSION_CHECK !== '0'
    ) {
      throw new Error(
        `Native binding package version mismatch, expected ${packageVersion} but got ${bindingPackageVersion}. You can reinstall dependencies to fix this issue.`,
      )
    }
    return binding
  } catch (err) {
    loadErrors.push(err)
    return null
  }
}

function requireBinding(tuple) {
  return requireLocal(tuple) || requirePackage(tuple)
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
    if (process.arch === 'x64') return requireBinding('win32-x64-msvc')
    if (process.arch === 'ia32') return requireBinding('win32-ia32-msvc')
    if (process.arch === 'arm64') return requireBinding('win32-arm64-msvc')
  } else if (process.platform === 'darwin') {
    if (process.arch === 'x64') return requireBinding('darwin-x64')
    if (process.arch === 'arm64') return requireBinding('darwin-arm64')
  } else if (process.platform === 'linux') {
    const musl = isMusl()
    if (process.arch === 'x64') return requireBinding(musl ? 'linux-x64-musl' : 'linux-x64-gnu')
    if (process.arch === 'arm64') return requireBinding(musl ? 'linux-arm64-musl' : 'linux-arm64-gnu')
    if (process.arch === 'riscv64' && !musl) return requireBinding('linux-riscv64-gnu')
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

export default nativeBinding
export const queryProxySettings = nativeBinding.queryProxySettings
export const setProxy = nativeBinding.setProxy
export const setPac = nativeBinding.setPac
export const disableProxy = nativeBinding.disableProxy
export const waitProxySettingsChange = nativeBinding.waitProxySettingsChange
export const ProxyGuard = nativeBinding.ProxyGuard
