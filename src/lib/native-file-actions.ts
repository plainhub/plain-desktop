import { invoke } from '@tauri-apps/api/core'
import { nativeGraphql } from '@/lib/api/native-client'

async function localPath(reference: string): Promise<string> {
  const paths = await invoke<Array<string | null>>('native_file_paths', { paths: [reference] })
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

export async function saveTextFileAs(name: string, contents: string): Promise<void> {
  const { save } = await import('@tauri-apps/plugin-dialog')
  const path = await save({ defaultPath: name.trim() || 'file.txt' })
  if (!path) return
  const response = await nativeGraphql<{
    data?: { writeTextFile: { path: string } | null }
    errors?: Array<{ message: string }>
  }>({
    query: 'mutation($path:String!,$content:String!){writeTextFile(path:$path,content:$content,overwrite:true){path}}',
    variables: { path, content: contents },
  }, 30000)
  if (response.errors?.length) throw new Error(response.errors[0].message)
  if (!response.data?.writeTextFile?.path) throw new Error('Text export did not produce a file')
}

export async function copyLocalFile(reference: string): Promise<void> {
  await invoke('copy_file_to_clipboard', { path: await localPath(reference) })
}
