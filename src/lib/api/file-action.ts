export enum FileAction {
  Preview = 'preview',
  Download = 'download',
  Probe = 'probe',
  Transcode = 'transcode',
}

export function fileQuery(action: FileAction, query = ''): string {
  const parameters = new URLSearchParams(query.startsWith('&') ? query.slice(1) : query)
  parameters.set('action', action)
  return '&' + parameters.toString()
}

export function withFileAction(url: string, action: FileAction): string {
  const parsed = new URL(url, 'http://localhost')
  if (parsed.pathname !== '/fs' && parsed.pathname !== '/proxyfs') return url
  parsed.searchParams.set('action', action)
  if (action !== FileAction.Preview) parsed.searchParams.delete('size')
  return /^https?:\/\//.test(url) ? parsed.toString() : parsed.pathname + parsed.search + parsed.hash
}
