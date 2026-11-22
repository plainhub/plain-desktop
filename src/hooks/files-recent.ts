import { useListError } from '@/hooks/list-error'
import { showActionError } from '@/hooks/show-action-error'
import { graphqlSubscriptions } from '@/lib/api/subscription-client'
import toast from '@/components/toaster'
import { onActivated, onDeactivated, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { storeToRefs } from 'pinia'
import { type IFile, enrichFile } from '@/lib/file'
import { useTempStore, type IUploadItem } from '@/stores/temp'
import { generateRecordId } from '@/lib/record-id.generated'
import { initMutation, setTempValueGQL } from '@/lib/api/mutation'
import { initLazyQuery, recentFilesGQL } from '@/lib/api/query'
import emitter from '@/plugins/eventbus'
import type { IFileDeletedEvent, IFileRenamedEvent } from '@/lib/interfaces'
import { createRefreshScheduler } from '@/lib/refresh-scheduler'
import { getIsPhone } from '@/hooks/device'
import { useFilePreviewList } from '@/hooks/file-preview-list'

export function useFilesRecent() {
  const { t } = useI18n()
  const isPhone = getIsPhone()
  const { app } = storeToRefs(useTempStore())
  const items = ref<IFile[]>([])
  const list = useFilePreviewList(items)
  const { selectedIds, clearSelection, total } = list.selection
  const { downloadFile, downloadFiles, downloadDir } = list
  const { listError, setListError } = useListError(items, total)

  const { mutate: setTempValue, onDone: setTempValueDone } = initMutation({ document: setTempValueGQL }, showActionError)
  setTempValueDone((r: any) => { downloadFiles(r.data.setTempValue.key); clearSelection() })

  const { loading, fetch } = initLazyQuery({
    handle: async (data: any, error: string) => {
      setListError(error)
      if (!error) {
        const files = data.recentFiles.map((item: any) => enrichFile(item))
        items.value = files
        total.value = files.length
      }
    },
    document: recentFilesGQL,
  })

  const getSelectedItems = () => {
    const selected = items.value.filter((it) => selectedIds.value.includes(it.id))
    if (selected.length === 0) { toast(t('select_first'), 'error'); return null }
    return selected
  }

  const downloadSelectedItem = () => {
    const selected = getSelectedItems()
    if (!selected || selected.length !== 1) return
    const item = selected[0]
    item.isDir ? downloadDir(item.path) : downloadFile(item.path)
    clearSelection()
  }

  const downloadEachItems = async () => {
    const selected = getSelectedItems()
    if (!selected) return
    for (const it of selected) {
      if (it.isDir) downloadDir(it.path); else downloadFile(it.path)
      await new Promise((resolve) => setTimeout(resolve, 250))
    }
    clearSelection()
  }

  const downloadZipItems = () => {
    if (!getSelectedItems()) return
    setTempValue({ key: generateRecordId(), value: JSON.stringify(selectedIds.value.map((it: string) => ({ path: it }))) })
  }

  const uploadRefresh = createRefreshScheduler(() => fetch())
  const uploadTaskDoneHandler = (r: IUploadItem) => { if (r.status === 'done') uploadRefresh.schedule() }
  const fileDeletedHandler = (event: IFileDeletedEvent) => {
    const previous = items.value
    const remaining = previous.filter((item) => item.id !== event.item.id)
    if (remaining.length === previous.length) return
    items.value = remaining
    total.value = remaining.length
  }
  const fileRenamedHandler = (_event: IFileRenamedEvent) => fetch()
  const accessUpdatedHandler = () => fetch()
  let stopPermissionsUpdated: (() => void) | undefined

  onActivated(() => {
    fetch()
    emitter.on('upload_task_done', uploadTaskDoneHandler)
    emitter.on('file_deleted', fileDeletedHandler)
    emitter.on('file_renamed', fileRenamedHandler)
    stopPermissionsUpdated = graphqlSubscriptions.subscribe('accessUpdated', accessUpdatedHandler)
  })

  onDeactivated(() => {
    uploadRefresh.dispose()
    emitter.off('upload_task_done', uploadTaskDoneHandler)
    emitter.off('file_deleted', fileDeletedHandler)
    emitter.off('file_renamed', fileRenamedHandler)
    stopPermissionsUpdated?.()
  })

  return { app, items, list, listError, loading, isPhone, downloadSelectedItem, downloadEachItems, downloadZipItems }
}
