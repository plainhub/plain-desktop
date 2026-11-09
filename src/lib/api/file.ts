import { FileAction, fileQuery, withFileAction } from './file-action'
import { arrayBufferToHex } from '../strutil'
import { getProxyUrl, getApiHeaders, getUrlToken } from './api'
import { encryptResourcePayload, resourceId, type Resource, type ResourceScope } from './resource-link'
import { isLocalMode } from '../device/local-mode'
import toast, { toastWithAction } from '@/components/toaster'
import { downloadToDir, downloadAs as downloadAsTo, revealInFolder, type DownloadReference } from '@/lib/download-dir'
import { nativeResourceUrl } from '@/hooks/native-resource-url'

declare global {
  interface Window {
    fileIdMap: Map<string, string>
  }
}

export function notId(id: string) {
  const l = id.toLowerCase()
  return l.startsWith('https://') || l.startsWith('http://') || l.startsWith('blob:')
}

export function getFileUrl(id: string, query: string = '', action: FileAction = FileAction.Preview) {
  if (notId(id)) {
    return id
  }

  return getProxyUrl(`/fs?id=${encodeURIComponent(id)}${fileQuery(action, query)}`)
}

// Reference failures remain errors; an empty native URL selects an unmigrated preview transport.
export async function resolveFileUrlByPath(key: Uint8Array | null, path: string, size?: number, query: string = ''): Promise<string> {
  if (!path) {
    return ''
  }
  const native = await nativeResourceUrl(path, size, query)
  if (native) {
    return native
  }
  if (!key) {
    return ''
  }
  return getFileUrl(getFileId(key, path), query)
}

export function getFileUrlByPath(key: Uint8Array | null, path: string) {
  if (!path || !key) {
    return ''
  }
  return getFileUrl(getFileId(key, path))
}

const _recentDownloads = new Set<string>()

export function download(url: string, name: string, chat = false, reference?: DownloadReference) {
  url = withFileAction(url, FileAction.Download, chat)
  // Prevent duplicate downloads triggered by rapid double-clicks.
  if (_recentDownloads.has(url)) {
    return
  }
  _recentDownloads.add(url)
  setTimeout(() => _recentDownloads.delete(url), 1000)

  // In Tauri, WKWebView ignores <a download> for remote URLs (http/https):
  // fetch the bytes and save them into the system Downloads folder.
  // Blob URLs (local recorded media) still work with <a download>.
  if (__IS_TAURI__ && !url.startsWith('blob:')) {
    void saveToFile(url, name, reference)
    return
  }

  const link = document.createElement('a')
  if (typeof link.download === 'string') {
    link.href = url
    link.download = name

    //Firefox requires the link to be in the body
    document.body.appendChild(link)

    link.click()

    //remove the link when done
    document.body.removeChild(link)
  } else {
    window.open(url)
  }
}

let _toastTimer: ReturnType<typeof setTimeout> | undefined
let _lastDownloadedPath = ''

// Bursts of downloads (e.g. "download individually") share one toast that
// fires after the last completion and reveals the most recent file.
function notifyDownloaded(path: string) {
  _lastDownloadedPath = path
  clearTimeout(_toastTimer)
  _toastTimer = setTimeout(() => {
    void (async () => {
      const { i18n } = await import('@/plugins/i18n')
      const { t } = i18n.global
      const dir = _lastDownloadedPath.slice(0, _lastDownloadedPath.lastIndexOf('/'))
      toastWithAction(t('downloaded_to', { dir }), t('show_in_folder'), () => {
        void revealInFolder(_lastDownloadedPath)
      })
    })()
  }, 600)
}

async function handleSaveError(error: unknown) {
  console.error(error)
  const { i18n } = await import('@/plugins/i18n')
  toast(i18n.global.t('download_failed'), 'error')
}

async function saveToFile(url: string, name: string, reference?: DownloadReference) {
  try {
    notifyDownloaded(await downloadToDir(url, name, reference))
  } catch (error) {
    await handleSaveError(error)
  }
}

// Desktop "Save As…": native save dialog (defaults to Downloads/name),
// web falls back to the browser's own download.
export function downloadAs(url: string, name: string, reference?: DownloadReference) {
  url = withFileAction(url, FileAction.Download)
  if (!__IS_TAURI__ || url.startsWith('blob:')) {
    download(url, name)
    return
  }
  void (async () => {
    try {
      const path = await downloadAsTo(url, name, reference)
      if (path) notifyDownloaded(path)
    } catch (error) {
      await handleSaveError(error)
    }
  })()
}

export function downloadFromString(content: string, mimeType: string, fileName: string) {
  const blob = new Blob([content], { type: mimeType })
  const url = URL.createObjectURL(blob)
  download(url, fileName)
}

export function getFileName(path: string | undefined | null) {
  if (!path) return ''
  return path.substring(path.lastIndexOf('/') + 1)
}

export async function getFileHash(f: File) {
  return arrayBufferToHex(await crypto.subtle.digest('SHA-256', await f.arrayBuffer()))
}

export function tokenToKey(token: string): Uint8Array {
  return Uint8Array.from(atob(token), (c) => c.charCodeAt(0))
}

export function encryptUrlParams(key: Uint8Array | null, params: string, purpose = 'zip-files') {
  if (!key) {
    return ''
  }

  return encryptResourcePayload(key, currentResourceScope(), purpose, params)
}

export function getChatFileId(key: Uint8Array | null, path: string): string {
  if (!path.startsWith('fid:') || !isLocalMode()) return getFileId(key, path)
  return getFileId(tokenToKey(getUrlToken()), path, '', { mode: 'c', id: getApiHeaders()['c-id'] })
}

export function getChatAttachmentUrl(
  sessionKey: Uint8Array | null,
  peerId: string,
  reference: string,
  query = '',
): string {
  const key = isLocalMode() ? tokenToKey(getUrlToken()) : sessionKey
  if (!key || !peerId || !reference.startsWith('fsid:')) return ''
  const scope = { mode: 'c' as const, id: getApiHeaders()['c-id'] }
  const id = encryptResourcePayload(key, scope, `chat-attachment:${peerId}`, reference)
  return getProxyUrl(`/proxy/${encodeURIComponent(peerId)}/fs?id=${encodeURIComponent(id)}${fileQuery(FileAction.Preview, query)}`)
}

export function getFinalPath(appDir: string, path: string) {
  if (path.startsWith('app://')) {
    return appDir + '/' + path.replace('app://', '')
  } else if (path.startsWith('fid:')) {
    const hash = path.replace('fid:', '')
    return `${appDir}/${hash.substring(0, 2)}/${hash.substring(2, 4)}/${hash}`
  }

  return path
}

function currentResourceScope(): ResourceScope {
  return isLocalMode() ? { mode: 'g' } : { mode: 'c', id: getApiHeaders()['c-id'] }
}

export function getFileId(key: Uint8Array | null, path: string, mediaId: string = '', scope?: ResourceScope, foregroundThumbnail = false) {
  if (!path || !key) {
    return ''
  }

  const loPath = path.toLowerCase()
  if (loPath.startsWith('https://') || loPath.startsWith('http://')) {
    return path
  }

  const resourceScope = scope ?? currentResourceScope()
  const resource: Resource = mediaId
    ? { type: foregroundThumbnail ? 'FOREGROUND_MEDIA_THUMBNAIL' : 'MEDIA', id: mediaId }
    : path.startsWith('fid:')
      ? { type: 'APP_FILE', id: path.slice(4) }
      : path.startsWith('pkgicon://')
        ? { type: 'PACKAGE_ICON', id: path.slice(10) }
        : { type: 'FILE', id: path }
  const cacheKey = JSON.stringify([resourceScope, Array.from(key), resource])
  const fileIdMap = window.fileIdMap || (window.fileIdMap = new Map<string, string>())
  const cached = fileIdMap.get(cacheKey)
  if (cached) return cached
  const id = resourceId(key, resourceScope, resource)
  fileIdMap.set(cacheKey, id)
  return id
}

export function getFileExtension(filePath: string) {
  const lastDotIndex = filePath.lastIndexOf('.')
  if (lastDotIndex === -1) {
    return ''
  }
  // fix hidden file extension
  const lastSlashIndex = filePath.lastIndexOf('/')
  if (lastSlashIndex > lastDotIndex) {
    return ''
  }

  return filePath.substring(lastDotIndex + 1).toLowerCase()
}
