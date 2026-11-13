import { type Ref } from 'vue'
import { useI18n } from 'vue-i18n'
import type { IDoc } from '@/lib/interfaces'
import type { IFile } from '@/lib/file'
import { useFileOpen } from '@/hooks/file-open'
import { openModal } from '@/components/modal'
import EditValueModal from '@/components/EditValueModal.vue'
import DeleteFileConfirm from '@/components/DeleteFileConfirm.vue'
import { initMutation, renameFileGQL, copyFileToDirectoryGQL } from '@/lib/api/mutation'
import { useDownload } from '@/hooks/files'
import { getFileUrlByPath, getFileName } from '@/lib/api/file'
import { getTextFileId } from '@/lib/api/text-file'
import { arrayRemove } from '@/lib/array'
import { openUrl } from '@/lib/browser'

export function useDocsActions(
  items: Ref<IDoc[]>,
  selectedIds: Ref<string[]>,
  clearSelection: () => void,
  fetch: () => void,
  urlTokenKey: Ref<Uint8Array | null>,
) {
  const { t } = useI18n()
  const { downloadFile } = useDownload(urlTokenKey)

  const { mutate: copyMutate, onDone: copyDone } = initMutation({ document: copyFileToDirectoryGQL })
  copyDone(() => fetch())

  const { openFile } = useFileOpen<IDoc>({
    items,
    getName: (item) => getFileName(item.path),
    openTextFile: (item) => {
      const fileId = getTextFileId(urlTokenKey.value, item.path)
      openUrl(`/text-file?id=${encodeURIComponent(fileId)}`)
    },
    openBrowserFile: (item) => openUrl(getFileUrlByPath(urlTokenKey.value, item.path)),
    viewMedia: (_, item) => downloadFile(item.path, item.title),
    download: (item) => downloadFile(item.path, item.title),
  })

  function deleteItem(item: IDoc) {
    openModal(DeleteFileConfirm, {
      files: [{ path: item.path, name: item.title }] as unknown as IFile[],
      onDone: () => { arrayRemove(items.value, (it: IDoc) => it.id === item.id) },
      onError: fetch,
    })
  }

  function renameItem(item: IDoc) {
    openModal(EditValueModal, {
      title: t('rename'),
      value: item.title,
      mutation: () => initMutation({ document: renameFileGQL }),
      getVariables: (name: string) => ({ path: item.path, name }),
      done: () => fetch(),
    })
  }

  function duplicateItem(item: IDoc) {
    const dir = item.path.substring(0, item.path.lastIndexOf('/'))
    copyMutate({ src: item.path, directory: dir, name: item.title })
  }

  return { downloadFile, openFile, deleteItem, renameItem, duplicateItem }
}
