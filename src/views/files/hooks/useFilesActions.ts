import { showActionError } from '@/hooks/show-action-error'
import { ref, type Ref, type ComputedRef } from 'vue'
import { fileNameError, type IFile } from '@/lib/file'
import type { IFileFilter } from '@/lib/interfaces'
import { openModal } from '@/components/modal'
import EditValueModal from '@/components/EditValueModal.vue'
import { generateRecordId } from '@/lib/record-id.generated'
import { getFileUrlByPath } from '@/lib/api/file'
import { initMutation, setTempValueGQL, addFavoriteFolderGQL, trashFilesGQL } from '@/lib/api/mutation'
import { useFilesTrash } from './useFilesTrash'
import emitter from '@/plugins/eventbus'
import toast from '@/components/toaster'
import { copyTextToClipboard } from '@/lib/clipboard'

interface UseFilesActionsOptions {
  items: Ref<IFile[]>
  visibleItems: ComputedRef<IFile[]>
  total: Ref<number>
  selectedIds: Ref<string[]>
  clearSelection: () => void
  retainSelection: (previousItems: IFile[]) => boolean
  filter: IFileFilter
  rootDir: ComputedRef<string>
  urlTokenKey: Ref<Uint8Array | null>
  t: (key: string, args?: any) => string
  fetch: () => void
  refetchMounts: () => void
  copy: (ids: string[]) => void
  cut: (ids: string[]) => void
  paste: (dir: string) => void
  downloadFile: (path: string) => void
  downloadDir: (path: string) => void
  downloadFiles: (key: string) => void
  createPath: Ref<string>
  createVariables: (value: string) => any
  createMutation: () => any
  renameItem: Ref<IFile | undefined>
  renameMutation: () => any
  renameVariables: (value: string) => any
  renameDone: (newName: string) => boolean
}

export function useFilesActions(opts: UseFilesActionsOptions) {
  const {
    items, visibleItems, total, selectedIds, clearSelection, retainSelection, filter, rootDir, urlTokenKey,
    t, fetch, refetchMounts, copy, cut, paste,
    downloadFile, downloadDir, downloadFiles,
    createPath, createVariables, createMutation,
    renameItem, renameMutation, renameVariables, renameDone,
  } = opts

  const { loading: downloadLoading, mutate: setTempValue, onDone: setTempValueDone } = initMutation({ document: setTempValueGQL }, showActionError)
  setTempValueDone((r: any) => { downloadFiles(r.data.setTempValue.key); clearSelection() })

  const { mutate: addFavoriteFolderMutation, onDone: addFavDone } = initMutation({
    document: addFavoriteFolderGQL,
  }, showActionError)
  addFavDone(() => emitter.emit('refetch_favorite_folders'))

  const onDeleted = (files: IFile[]) => {
    const removedIds = new Set(files.map((file) => file.id))
    const remaining = items.value.filter((item) => !removedIds.has(item.id))
    const removed = items.value.length - remaining.length
    if (removed) {
      const previous = visibleItems.value
      items.value = remaining
      total.value -= removed
      retainSelection(previous)
    }
    if (pendingTrashFiles.value.some((file) => removedIds.has(file.id))) cancelTrashItems()
    refetchMounts()
  }

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
      it.isDir ? downloadDir(it.path) : downloadFile(it.path)
      await new Promise((resolve) => setTimeout(resolve, 250))
    }
    clearSelection()
  }

  const downloadZipItems = () => {
    if (!getSelectedItems()) return
    setTempValue({ key: generateRecordId(), value: JSON.stringify(selectedIds.value.map((p) => ({ path: p }))) })
  }

  const { mutate: trashFiles } = initMutation({ document: trashFilesGQL }, showActionError)
  const { trashingIds, collapsingIds, bulkTrashing, isTrashing, startTrash } = useFilesTrash({ t, mutate: trashFiles, onDeleted, refresh: () => { fetch(); refetchMounts() } })
  const pendingTrashFiles = ref<IFile[]>([])
  const confirmingTrash = ref(false)
  const trashCount = ref(0)

  const trashItems = () => {
    const selected = items.value.filter((it) => selectedIds.value.includes(it.id) && !isTrashing(it.id))
    if (selected.length === 0) { toast(t('select_first'), 'error'); return }
    pendingTrashFiles.value = selected
    trashCount.value = selected.length
    confirmingTrash.value = true
  }
  const confirmTrashItems = () => {
    const files = pendingTrashFiles.value
    pendingTrashFiles.value = []
    confirmingTrash.value = false
    startTrash(files, { bulk: true })
  }
  const cancelTrashItems = () => { confirmingTrash.value = false; pendingTrashFiles.value = [] }

  const trashItem = (item: IFile) => {
    if (isTrashing(item.id)) return
    startTrash([item])
  }
  const copyItems = () => { copy(selectedIds.value); clearSelection() }
  const cutItems = () => { cut(selectedIds.value); clearSelection() }
  const pasteDir = () => { paste(filter.parent) }
  const duplicateItem = (item: IFile) => { copy([item.id]); paste(filter.parent) }
  const cutItem = (item: IFile) => { cut([item.id]) }
  const copyItem = (item: IFile) => { copy([item.id]) }
  const pasteItem = (item: IFile) => { paste(item.path) }

  const copyLinkItem = async (item: IFile) => {
    const url = getFileUrlByPath(urlTokenKey.value, item.path)
    const ok = await copyTextToClipboard(url)
    toast(ok ? t('link_copied') : t('copy_failed'), ok ? undefined : 'error')
  }

  const renameItemClick = (item: IFile) => {
    renameItem.value = item
    openModal(EditValueModal, {
      title: t('rename'), placeholder: t('name'), value: item.name,
      validate: fileNameError,
      mutation: renameMutation, getVariables: renameVariables, done: renameDone,
    })
  }

  const createDir = () => {
    createPath.value = filter.parent
    openModal(EditValueModal, {
      title: t('create_folder'), placeholder: t('name'),
      validate: fileNameError,
      mutation: createMutation, getVariables: createVariables,
    })
  }

  const addToFavoritesClick = (item: IFile) => {
    if (!item.isDir) return
    addFavoriteFolderMutation({ rootPath: rootDir.value, fullPath: item.path })
      .then(result => { if (result) toast(t('added')) })
  }

  return {
    downloadLoading, downloadSelectedItem, downloadEachItems, downloadZipItems, trashItems, trashItem,
    confirmingTrash, trashCount, confirmTrashItems, cancelTrashItems,
    trashingIds, collapsingIds, bulkTrashing, isTrashing,
    copyItems, cutItems, pasteDir, duplicateItem, cutItem, copyItem, pasteItem,
    copyLinkItem, renameItemClick, createDir, addToFavoritesClick, onDeleted,
  }
}
