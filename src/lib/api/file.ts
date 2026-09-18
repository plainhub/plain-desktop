import { arrayBufferToHex } from '../strutil'
import { buildUrl } from '../url'
import { getPeerIp } from '../device/login-peers'
import { getApiBaseUrl, getProxyUrl } from './api'
import { chachaEncrypt, bitArrayToBase64 } from './crypto'
import toast, { toastWithAction } from '@/components/toaster'
import { downloadToDir, getDownloadDir, revealInFolder } from '@/lib/download-dir'

declare global {
  interface Window {
    fileIdMap: Map<string, string>
  }
}

export function notId(id: string) {
  const l = id.toLowerCase()
  return l.startsWith('https://') || l.startsWith('http://') || l.startsWith('blob:')
}

export function getFileUrl(id: string, query: string = '') {
  if (notId(id)) {
    return id
  }

  return getProxyUrl(`/fs?id=${encodeURIComponent(id)}${query}`)
}

export function getFileUrlByPath(key: Uint8Array | null, path: string) {
  if (!path || !key) {
    return ''
  }
  return getFileUrl(getFileId(key, path))
}

const _recentDownloads = new Set<string>()

export function download(url: string, name: string) {
  // Prevent duplicate downloads triggered by rapid double-clicks.
  if (_recentDownloads.has(url)) {
    return
  }
  _recentDownloads.add(url)
  setTimeout(() => _recentDownloads.delete(url), 1000)

  // In Tauri, WKWebView ignores <a download> for remote URLs (http/https):
  // fetch the bytes and write them into the saved download folder (chosen
  // via a native dialog on first use). Blob URLs (local recorded media)
  // still work with <a download>.
  if (__IS_TAURI__ && !url.startsWith('blob:')) {
    void saveToFile(url, name)
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
      toastWithAction(t('downloaded_to', { dir: getDownloadDir() }), t('show_in_folder'), () => {
        void revealInFolder(_lastDownloadedPath)
      })
    })()
  }, 600)
}

async function saveToFile(url: string, name: string) {
  try {
    const path = await downloadToDir(url, name)
    if (path) notifyDownloaded(path)
  } catch (error) {
    console.error(error)
    const { i18n } = await import('@/plugins/i18n')
    toast(i18n.global.t('download_failed'), 'error')
  }
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

export function encryptUrlParams(key: Uint8Array | null, params: string) {
  if (!key) {
    return ''
  }

  const enc = chachaEncrypt(key, params)
  return bitArrayToBase64(enc)
}

/**
 * Build a /proxyfs URL that proxies a peer's file through the local app server.
 * Avoids CORS issues when the peer uses a self-signed HTTPS certificate.
 * @param peerFileId - the raw peer-side encrypted file ID (from `fsid:<id>` URI)
 */
export function getPeerProxyUrl(
  urlTokenKey: Uint8Array | null,
  peer: { ip: string; port: number },
  peerFileId: string,
  query: string = '',
): string {
  if (!urlTokenKey || !peer?.ip || !peer?.port || !peerFileId) {
    return ''
  }
  const peerUrl = buildUrl('https', getPeerIp(peer), peer.port, `/fs?id=${encodeURIComponent(peerFileId)}${query}`)
  const encrypted = encryptUrlParams(urlTokenKey, peerUrl)
  return `${getApiBaseUrl()}/proxyfs?id=${encodeURIComponent(encrypted)}`
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

export function getFileId(key: Uint8Array | null, path: string, mediaId: string = '') {
  if (!path || !key) {
    return ''
  }

  const loPath = path.toLowerCase()
  if (loPath.startsWith('https://') || loPath.startsWith('http://')) {
    return path
  }

  const fileIdMap = window.fileIdMap || new Map<string, string>()
  if (fileIdMap.has(path)) {
    return fileIdMap.get(path) ?? ''
  }

  const enc = chachaEncrypt(key, mediaId ? JSON.stringify({ path, mediaId }) : path)
  const id = bitArrayToBase64(enc)
  fileIdMap.set(path, id)
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
