import { invoke } from '@tauri-apps/api/core'
import { nativeGraphql } from '@/lib/api/native-client'

async function localPath(reference: string): Promise<string> {
  const response = await nativeGraphql<{ data?: { filePaths: Array<string | null> }; errors?: { message: string }[] }>({
    query: 'query($paths: [String!]!) { filePaths(paths: $paths) }',
    variables: { paths: [reference] },
  }, 30000)
  if (response.errors?.length) throw new Error(response.errors[0].message)
  const paths = response.data?.filePaths
  const path = Array.isArray(paths) ? paths[0] : null
  if (typeof path !== 'string' || !path) throw new Error('File reference could not be resolved')
  return path
}

export async function revealLocalFile(reference: string): Promise<void> {
  await invoke('reveal_file', { path: await localPath(reference) })
}

export async function saveLocalFileAs(reference: string, name: string): Promise<void> {
  await invoke('save_file_as', { path: await localPath(reference), name })
}

export async function copyLocalFile(reference: string): Promise<void> {
  await invoke('copy_file_to_clipboard', { path: await localPath(reference) })
}
