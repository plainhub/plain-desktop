import { ref } from 'vue'
import type { IFile } from '@/lib/file'
import toast from '@/components/toaster'

export const FILE_DELETE_COLLAPSE_MS = 220

interface UseFilesDeleteOptions {
  t: (key: string, args?: any) => string
  mutate: (variables?: any) => Promise<any>
  onDeleted: (files: IFile[]) => void
}

export function useFilesDelete(opts: UseFilesDeleteOptions) {
  const deletingIds = ref<string[]>([])
  const collapsingIds = ref<string[]>([])
  const bulkDeleting = ref(false)

  const isDeleting = (id: string) => deletingIds.value.includes(id) || collapsingIds.value.includes(id)

  const startDelete = async (files: IFile[], options?: { bulk?: boolean }) => {
    if (files.length === 0) return
    if (options?.bulk) bulkDeleting.value = true
    const ids = files.map((f) => f.id)
    deletingIds.value = [...deletingIds.value, ...ids]
    let result: any
    try {
      result = await opts.mutate({ paths: files.map((it) => it.path) })
    } catch {
      result = undefined
    } finally {
      bulkDeleting.value = false
    }
    if (result != null) {
      collapsingIds.value = [...collapsingIds.value, ...ids]
      deletingIds.value = deletingIds.value.filter((id) => !ids.includes(id))
      setTimeout(() => {
        opts.onDeleted(files)
        collapsingIds.value = collapsingIds.value.filter((id) => !ids.includes(id))
      }, FILE_DELETE_COLLAPSE_MS)
      toast(files.length > 1 ? opts.t('deleted_n_items', { n: files.length }) : opts.t('deleted'))
    } else {
      deletingIds.value = deletingIds.value.filter((id) => !ids.includes(id))
      toast(opts.t('delete_failed_name', { name: files.map((f) => f.name).join(', ') }), 'error')
    }
  }

  return { deletingIds, collapsingIds, bulkDeleting, isDeleting, startDelete }
}
