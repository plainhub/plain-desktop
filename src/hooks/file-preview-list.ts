import { onActivated, onDeactivated, ref, type Ref } from 'vue'
import { storeToRefs } from 'pinia'
import { useTempStore } from '@/stores/temp'
import { getFileUrlByPath } from '@/lib/api/file'
import { getTextFileId } from '@/lib/api/text-file'
import { openUrl } from '@/lib/browser'
import type { IFile } from '@/lib/file'
import type { ISource } from '@/components/lightbox/types'
import { useDownload, useView } from '@/hooks/files'
import { useSelectable } from '@/hooks/list'
import { useFilesKeyEvents } from '@/hooks/key-events'
import { useOpenMedia } from '@/hooks/open-media'
import { useFileOpen } from '@/hooks/file-open'

export function useFilePreviewList(items: Ref<IFile[]>, readOnly = false) {
  const { urlTokenKey } = storeToRefs(useTempStore())
  const downloads = useDownload(urlTokenKey)
  const selection = useSelectable(items)
  const { keyDown, keyUp } = useFilesKeyEvents(selection.selectAll, selection.clearSelection, () => {})
  onActivated(() => {
    window.addEventListener('keydown', keyDown)
    window.addEventListener('keyup', keyUp)
  })
  onDeactivated(() => {
    window.removeEventListener('keydown', keyDown)
    window.removeEventListener('keyup', keyUp)
  })

  const { open } = useOpenMedia()
  const { view } = useView(ref<ISource[]>([]), (sources, index) => open(index, sources, readOnly))
  const { openFile } = useFileOpen({
    items,
    getName: (item) => item.name,
    openTextFile: (item) => {
      const id = getTextFileId(urlTokenKey.value, item.path)
      openUrl(`/text-file?id=${encodeURIComponent(id)}`)
    },
    openBrowserFile: (item) => window.open(getFileUrlByPath(urlTokenKey.value, item.path), '_blank'),
    viewMedia: view,
    download: (item) => downloads.downloadFile(item.path, item.name),
  })

  return {
    selection,
    ...downloads,
    openFile: (item: IFile) => {
      if (!readOnly || !item.isDir) openFile(item)
    },
  }
}

export type FilePreviewList = ReturnType<typeof useFilePreviewList>
