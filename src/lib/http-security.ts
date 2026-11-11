export function isHttpBrowser(protocol: string, nativeApp: boolean): boolean {
  return !nativeApp && protocol === 'http:'
}

export function httpsPageUrl(currentUrl: string, port: unknown): string {
  if (typeof port !== 'number' || !Number.isInteger(port) || port < 1 || port > 65535) return ''
  const url = new URL(currentUrl)
  if (url.protocol !== 'http:') return ''
  url.protocol = 'https:'
  url.port = String(port)
  url.username = ''
  url.password = ''
  return url.href
}
