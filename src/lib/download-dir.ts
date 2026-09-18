import { get as prefsGet, set as prefsSet } from '@/lib/prefs'
import { getProxyUrl } from '@/lib/api/api'

const DOWNLOAD_DIR_KEY = 'download_dir'

export function getDownloadDir(): string {
  return prefsGet<string>(DOWNLOAD_DIR_KEY, '') || ''
}

export function setDownloadDir(dir: string): void {
  prefsSet(DOWNLOAD_DIR_KEY, dir)
}

export async function chooseDownloadDir(): Promise<string | null> {
  const { open } = await import('@tauri-apps/plugin-dialog')
  const current = getDownloadDir()
  const selected = await open({ directory: true, multiple: false, defaultPath: current || undefined })
  const dir = (Array.isArray(selected) ? selected[0] : selected) || ''
  if (!dir) return null
  setDownloadDir(dir)
  return dir
}

export function joinPath(dir: string, name: string): string {
  return dir.replace(/[\\/]+$/, '') + '/' + name
}

export async function uniqueDownloadPath(dir: string, name: string, exists: (path: string) => Promise<boolean>): Promise<string> {
  let path = joinPath(dir, name)
  if (!(await exists(path))) return path
  const dot = name.lastIndexOf('.')
  const base = dot > 0 ? name.slice(0, dot) : name
  const ext = dot > 0 ? name.slice(dot) : ''
  for (let i = 2; ; i++) {
    path = joinPath(dir, `${base} (${i})${ext}`)
    if (!(await exists(path))) return path
  }
}

export async function downloadToDir(url: string, name: string): Promise<string | null> {
  let dir = getDownloadDir()
  if (!dir) {
    dir = (await chooseDownloadDir()) || ''
    if (!dir) return null
  }
  const { exists, writeFile } = await import('@tauri-apps/plugin-fs')
  const path = await uniqueDownloadPath(dir, name, exists)
  const { pathname, search } = new URL(url)
  const res = await fetch(getProxyUrl(pathname + search))
  if (!res.ok) throw new Error(`download failed: HTTP ${res.status}`)
  if (res.body) {
    await writeFile(path, res.body)
  } else {
    await writeFile(path, new Uint8Array(await res.arrayBuffer()))
  }
  return path
}

export async function revealInFolder(path: string): Promise<void> {
  const { revealItemInDir } = await import('@tauri-apps/plugin-opener')
  await revealItemInDir(path)
}
