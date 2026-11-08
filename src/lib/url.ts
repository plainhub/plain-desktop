function isDefaultPort(scheme: string, port: number): boolean {
  return ((scheme === 'http' || scheme === 'ws') && port === 80) ||
    ((scheme === 'https' || scheme === 'wss') && port === 443)
}

export function buildUrl(scheme: string, host: string, port: number, path: string = ''): string {
  const portPart = isDefaultPort(scheme, port) ? '' : `:${port}`
  return `${scheme}://${host}${portPart}${path}`
}

export function applyScheme(scheme: string, hostWithPort: string): string {
  const m = hostWithPort.match(/^(.+):(\d+)$/)
  if (m && isDefaultPort(scheme, Number(m[2]))) return `${scheme}://${m[1]}`
  return `${scheme}://${hostWithPort}`
}

export function peerProxyPath(peerId: string, path: string): string {
  return `/proxy/${encodeURIComponent(peerId)}/${path.replace(/^\//, '')}`
}
