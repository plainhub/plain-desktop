import { showActionError } from '@/hooks/show-action-error'
import { type Ref } from 'vue'
import { useI18n } from 'vue-i18n'
import type { IDoc } from '@/lib/interfaces'
import { useFileOpen } from '@/hooks/file-open'
import { openModal } from '@/components/modal'
import EditValueModal from '@/components/EditValueModal.vue'
import { initMutation, copyFileToDirectoryGQL, deleteFilesGQL } from '@/lib/api/mutation'
import { useDownload, useRename } from '@/hooks/files'
import { getFileUrlByPath, getFileName } from '@/lib/api/file'
import { getTextFileId } from '@/lib/api/text-file'
import { arrayRemove } from '@/lib/array'
import { openUrl } from '@/lib/browser'

export function useDocsActions(
  items: Ref<IDoc[]>,
  urlTokenKey: Ref<Uint8Array | null>,
  refreshItems: () => void,
) {
  const { t } = useI18n()
  const { downloadFile } = useDownload(urlTokenKey)
  const { renameItem: renameTarget, renameMutation, renameVariables, renameDone } = useRename<IDoc>(refreshItems)

  const { mutate: copyMutate, onDone: copyDone } = initMutation({ document: copyFileToDirectoryGQL }, showActionError)
  copyDone(refreshItems)

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

  const { mutate: deleteFiles, onError: onDeleteError } = initMutation({ document: deleteFilesGQL }, showActionError)
  onDeleteError(refreshItems)

  async function deleteItem(item: IDoc): Promise<boolean> {
    if (!await deleteFiles({ paths: [item.path] })) return false
    arrayRemove(items.value, (it) => it.id === item.id)
    return true
  }

  function renameItem(item: IDoc) {
    const name = getFileName(item.path)
    renameTarget.value = item
    openModal(EditValueModal, {
      title: t('rename'),
      value: name,
      mutation: renameMutation,
      getVariables: renameVariables,
      done: renameDone,
    })
  }

  function duplicateItem(item: IDoc) {
    const dir = item.path.substring(0, item.path.lastIndexOf('/'))
    copyMutate({ src: item.path, directory: dir, name: item.title })
  }

  return { downloadFile, openFile, deleteItem, renameItem, duplicateItem }
}
