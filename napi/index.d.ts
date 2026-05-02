export interface Options {
  proxy?: string
  bypass?: string
  pacUrl?: string
  device?: string
  onlyActiveDevice?: boolean
  concurrent?: boolean
  useRegistry?: boolean
}

export interface ProxyInfo {
  enable: boolean
  sameForAll: boolean
  servers: string
  bypass: string
}

export interface PacInfo {
  enable: boolean
  url: string
}

export interface ProxyConfig {
  proxy: ProxyInfo
  pac: PacInfo
}

export function queryProxySettings(options?: Options): ProxyConfig
export function setProxy(options?: Options): void
export function setPac(options?: Options): void
export function disableProxy(options?: Options): void
