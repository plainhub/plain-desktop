import { getProxyUrl } from '@/lib/api/api'
import { nativeGraphql } from '@/lib/api/native-client'

export function joinPath(dir: string, name: string): string {
  return dir.replace(/[\\/]+$/, '') + '/' + name
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
  const { pathname, search } = new URL(url)
  const source = getProxyUrl(pathname + search)
  const { downloadDir } = await import('@tauri-apps/api/path')
  const response = await nativeGraphql<{
    data?: { copyFileToDirectory: string | null }
    errors?: Array<{ message: string }>
  }>({
    query: 'mutation($source:String!,$directory:String!,$name:String!){copyFileToDirectory(src:$source,directory:$directory,name:$name)}',
    variables: { source, directory: await downloadDir(), name },
  }, 24 * 60 * 60 * 1000)
  if (response.errors?.length) throw new Error(response.errors[0].message)
  const path = response.data?.copyFileToDirectory
  if (typeof path !== 'string' || !path) throw new Error('Download did not produce a file')
  return path
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
