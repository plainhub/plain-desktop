import { getProxyUrl } from '@/lib/api/api'
import { nativeGraphql, nativeViewScope, type NativeTarget } from '@/lib/api/native-client'
import { invoke } from '@tauri-apps/api/core'
import { isNativeResourceUrl, nativeResourceReference } from '@/hooks/native-resource-url'

export function joinPath(dir: string, name: string): string {
  return dir.replace(/[\\/]+$/, '') + '/' + name
}

export type DownloadReference = string | { path: string; target: NativeTarget }

function resourceSource(url: string, reference?: DownloadReference): { path: string; target: NativeTarget } | null {
  if (isNativeResourceUrl(url)) return nativeResourceReference(url)
  if (!reference || typeof reference === 'string' || /^(https?:|blob:|fsid:|content:|ph:)/i.test(reference.path)) return null
  return reference
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

export async function downloadToDir(url: string, name: string, reference?: DownloadReference): Promise<string> {
  const resource = resourceSource(url, reference)
  const source = resource ? '' : (() => {
    const { pathname, search } = new URL(url)
    return getProxyUrl(pathname + search)
  })()
  const { downloadDir } = await import('@tauri-apps/api/path')
  if (resource) {
    const scope = await nativeViewScope()
    return invoke<string>('native_resource_download', { ...resource, scope, directory: await downloadDir(), name })
  }
  const response = await nativeGraphql<{
    data?: { copyFileToDirectory: string }
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

export async function downloadAs(url: string, name: string, reference?: DownloadReference): Promise<string | null> {
  const resource = resourceSource(url, reference)
  if (resource) {
    return invoke<string | null>('native_resource_save_as', { ...resource, scope: await nativeViewScope(), name })
  }
  const { save } = await import('@/lib/tauri-plugins')
  const { downloadDir } = await import('@tauri-apps/api/path')
  const path = await save({ defaultPath: joinPath(await downloadDir(), name) })
  if (!path) return null
  return writeDownload(url, path)
}

export async function revealInFolder(path: string): Promise<void> {
  const { revealItemInDir } = await import('@/lib/tauri-plugins')
  await revealItemInDir(path)
}
