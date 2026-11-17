export enum FileAction {
  Preview = 'PREVIEW',
  Download = 'DOWNLOAD',
  Probe = 'PROBE',
  Transcode = 'TRANSCODE',
}

export function fileQuery(action: FileAction, query = ''): string {
  const parameters = new URLSearchParams(query.startsWith('&') ? query.slice(1) : query)
  parameters.set('action', action)
  return '&' + parameters.toString()
}

export function withFileAction(url: string, action: FileAction, _chat = false): string {
  const parsed = new URL(url, 'http://localhost')
  const resource = parsed.pathname.replace(/^\/proxy\/[^/]+/, '')
  if (resource !== '/fs') return url
  parsed.searchParams.set('action', action)
  if (action !== FileAction.Preview) parsed.searchParams.delete('size')
  return /^https?:\/\//.test(url) ? parsed.toString() : parsed.pathname + parsed.search + parsed.hash
}

export function chatAttachmentThumbnail(url: string): string {
  if (!url) return ''
  const parsed = new URL(url)
  parsed.searchParams.set('size', 'SM')
  return parsed.toString()
}
