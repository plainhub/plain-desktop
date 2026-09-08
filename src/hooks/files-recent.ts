import toast from '@/components/toaster'
import { onActivated, onDeactivated, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { storeToRefs } from 'pinia'
import { type IFile, enrichFile } from '@/lib/file'
import { getFileUrlByPath } from '@/lib/api/file'
import { useDownload, useView } from '@/hooks/files'
import { openModal } from '@/components/modal'
import DownloadMethodModal from '@/components/DownloadMethodModal.vue'
import { useTempStore, type IUploadItem } from '@/stores/temp'
import { shortUUID } from '@/lib/strutil'
import { initMutation, setTempValueGQL } from '@/lib/api/mutation'
import type { ISource } from '@/components/lightbox/types'
import { useSelectable } from '@/hooks/list'
import { useFilesKeyEvents } from '@/hooks/key-events'
import { initLazyQuery, recentFilesGQL } from '@/lib/api/query'
import emitter from '@/plugins/eventbus'
import type { IFileDeletedEvent, IFileRenamedEvent } from '@/lib/interfaces'
import { arrayRemove } from '@/lib/array'
import { getIsPhone } from '@/hooks/device'
import { useOpenMedia } from '@/hooks/open-media'
import { useFileOpen } from '@/hooks/file-open'

export function useFilesRecent() {
  const { t } = useI18n()
  const sources = ref([])
  const isPhone = getIsPhone()
  const tempStore = useTempStore()
  const { app, urlTokenKey } = storeToRefs(tempStore)
  const items = ref<IFile[]>([])

  const { selectedIds, allChecked, realAllChecked, clearSelection, toggleAllChecked, toggleSelect, total, checked, shiftEffectingIds, handleItemClick, handleMouseOver, selectAll, shouldSelect } =
    useSelectable(items)
  const { downloadFile, downloadFiles, downloadDir } = useDownload(urlTokenKey)
  const { open: openMedia } = useOpenMedia()
  const { view } = useView(sources, (s: ISource[], index: number) => {
    const target = s[index]
    if (!target) return
    openMedia(index, s)
  })
  const { keyDown: pageKeyDown, keyUp: pageKeyUp } = useFilesKeyEvents(selectAll, clearSelection, () => {})

  const imageErrorIds = ref<string[]>([])
  const extensionImageErrorIds = ref<string[]>([])
  const onImageError = (id: string) => imageErrorIds.value.push(id)
  const onExtensionImageError = (id: string) => extensionImageErrorIds.value.push(id)

  const { mutate: setTempValue, onDone: setTempValueDone } = initMutation({ document: setTempValueGQL })
  setTempValueDone((r: any) => { downloadFiles(r.data.setTempValue.key); clearSelection() })

  const { openFile } = useFileOpen<IFile>({
    items,
    openBrowserFile: (item) => window.open(getFileUrlByPath(urlTokenKey.value, item.path), '_blank'),
    viewMedia: view,
    download: (item) => downloadFile(item.path),
  })

  const { loading, fetch } = initLazyQuery({
    handle: async (data: any, error: string) => {
      if (error) toast(t(error), 'error')
      else {
        const files = data.recentFiles.map((item: any) => enrichFile(item))
        items.value = files
        total.value = files.length
      }
    },
    document: recentFilesGQL,
  })

  const downloadItems = () => {
    const selected = items.value.filter((it) => selectedIds.value.includes(it.id))
    if (selected.length === 0) { toast(t('select_first'), 'error'); return }
    if (selected.length === 1 && !selected[0].isDir) { downloadFile(selected[0].path); clearSelection(); return }
    openModal(DownloadMethodModal, {
      onEach: async () => {
        for (const it of selected) {
          if (it.isDir) downloadDir(it.path); else downloadFile(it.path)
          await new Promise((resolve) => setTimeout(resolve, 250))
        }
        clearSelection()
      },
      onZip: () => {
        setTempValue({ key: shortUUID(), value: JSON.stringify(selectedIds.value.map((it: string) => ({ path: it }))) })
      },
    })
  }

  const uploadTaskDoneHandler = (r: IUploadItem) => { if (r.status === 'done') setTimeout(() => fetch(), 1000) }
  const fileDeletedHandler = (event: IFileDeletedEvent) => { arrayRemove(items.value, (it: IFile) => it.id === event.item.id); clearSelection() }
  const fileRenamedHandler = (_event: IFileRenamedEvent) => fetch()

  onActivated(() => {
    fetch()
    emitter.on('upload_task_done', uploadTaskDoneHandler)
    emitter.on('file_deleted', fileDeletedHandler)
    emitter.on('file_renamed', fileRenamedHandler)
    window.addEventListener('keydown', pageKeyDown)
    window.addEventListener('keyup', pageKeyUp)
  })

  onDeactivated(() => {
    emitter.off('upload_task_done', uploadTaskDoneHandler)
    emitter.off('file_deleted', fileDeletedHandler)
    emitter.off('file_renamed', fileRenamedHandler)
    window.removeEventListener('keydown', pageKeyDown)
    window.removeEventListener('keyup', pageKeyUp)
  })

  return {
    app, items, loading, isPhone,
    selectedIds, allChecked, realAllChecked, checked, total,
    shiftEffectingIds, shouldSelect, imageErrorIds, extensionImageErrorIds,
    toggleAllChecked, toggleSelect, handleItemClick, handleMouseOver,
    onImageError, onExtensionImageError, downloadFile, clickItem: openFile, downloadItems,
  }
}
