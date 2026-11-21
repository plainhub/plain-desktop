import { ref } from 'vue'
import type { IFile } from '@/lib/file'
import toast from '@/components/toaster'

export const FILE_TRASH_COLLAPSE_MS = 220

interface UseFilesTrashOptions {
  t: (key: string, args?: any) => string
  mutate: (variables?: any) => Promise<any>
  onDeleted: (files: IFile[]) => void
  refresh: () => void
}

export function useFilesTrash(opts: UseFilesTrashOptions) {
  const trashingIds = ref<string[]>([])
  const collapsingIds = ref<string[]>([])
  const bulkTrashing = ref(false)

  const isTrashing = (id: string) => trashingIds.value.includes(id) || collapsingIds.value.includes(id)

  const startTrash = async (files: IFile[], options?: { bulk?: boolean }) => {
    if (files.length === 0) return
    if (options?.bulk) bulkTrashing.value = true
    const ids = files.map((f) => f.id)
    trashingIds.value = [...trashingIds.value, ...ids]
    let result: any
    try {
      result = await opts.mutate({ paths: files.map((it) => it.path) })
    } catch {
      result = undefined
    } finally {
      if (options?.bulk) bulkTrashing.value = false
    }
    if (result?.data?.trashFiles?.affectedCount === files.length) {
      collapsingIds.value = [...collapsingIds.value, ...ids]
      trashingIds.value = trashingIds.value.filter((id) => !ids.includes(id))
      setTimeout(() => {
        opts.onDeleted(files)
        collapsingIds.value = collapsingIds.value.filter((id) => !ids.includes(id))
      }, FILE_TRASH_COLLAPSE_MS)
    } else {
      trashingIds.value = trashingIds.value.filter((id) => !ids.includes(id))
      opts.refresh()
      if (result) toast(opts.t('trash_failed_name', { name: files.map((f) => f.name).join(', ') }), 'error')
    }
  }

  return { trashingIds, collapsingIds, bulkTrashing, isTrashing, startTrash }
}
