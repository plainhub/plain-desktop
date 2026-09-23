import { getProxyUrl } from '@/lib/api/api'

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

async function writeDownload(url: string, path: string): Promise<string> {
  const { writeFile } = await import('@tauri-apps/plugin-fs')
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

export async function downloadToDir(url: string, name: string): Promise<string> {
  const { downloadDir } = await import('@tauri-apps/api/path')
  const { exists } = await import('@tauri-apps/plugin-fs')
  return writeDownload(url, await uniqueDownloadPath(await downloadDir(), name, exists))
}

export async function downloadAs(url: string, name: string): Promise<string | null> {
  const { save } = await import('@tauri-apps/plugin-dialog')
  const { downloadDir } = await import('@tauri-apps/api/path')
  const path = await save({ defaultPath: joinPath(await downloadDir(), name) })
  if (!path) return null
  return writeDownload(url, path)
}

export async function revealInFolder(path: string): Promise<void> {
  const { revealItemInDir } = await import('@tauri-apps/plugin-opener')
  await revealItemInDir(path)
}
