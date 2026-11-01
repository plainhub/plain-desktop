import { type Ref, type ComputedRef } from 'vue'
import { type IFile, isZipFile, joinZipPath } from '@/lib/file'
import { getFileUrlByPath, getFileId } from '@/lib/api/file'
import type { IFileFilter } from '@/lib/interfaces'
import { useMainStore } from '@/stores/main'
import { replacePath } from '@/plugins/router'
import { openUrl } from '@/lib/browser'
import { useFileOpen } from '@/hooks/file-open'

interface UseFilesNavigationOptions {
  filter: IFileFilter
  rootDir: ComputedRef<string>
  urlTokenKey: Ref<Uint8Array | null>
  buildQ: (filter: IFileFilter) => string
  clearSelection: () => void
  view: (items: IFile[], f: IFile) => void
  downloadFile: (path: string) => void
  items: Ref<IFile[]>
}

export function useFilesNavigation(opts: UseFilesNavigationOptions) {
  const { filter, rootDir, urlTokenKey, buildQ, clearSelection, view, downloadFile, items } = opts
  const mainStore = useMainStore()

  function getUrl(q: string) {
    return q ? `/files?q=${q}` : `/files`
  }

  function navigateToDir(dir: string) {
    clearSelection()
    filter.parent = dir
    filter.text = ''
    replacePath(mainStore, getUrl(buildQ(filter)))
  }

  function toggleShowHidden() {
    filter.showHidden = !filter.showHidden
    mainStore.fileShowHidden = filter.showHidden
    replacePath(mainStore, getUrl(buildQ(filter)))
  }

  const { openFile } = useFileOpen<IFile>({
    items,
    openTextFile: (item) => {
      const fileId = getFileId(urlTokenKey.value, item.path)
      openUrl(`/text-file?id=${encodeURIComponent(fileId)}`)
    },
    openBrowserFile: (item) => openUrl(getFileUrlByPath(urlTokenKey.value, item.path)),
    viewMedia: view,
    download: (item) => downloadFile(item.path),
  })

  function clickItem(item: IFile) {
    if (item.isDir) { navigateToDir(item.path); return }
    if (isZipFile(item.name)) { navigateToDir(joinZipPath(item.path, '')); return }
    openFile(item)
  }

  function viewItem(event: Event, item: IFile) {
    if (item.isDir) return
    event.stopPropagation()
    if (isZipFile(item.name)) { navigateToDir(joinZipPath(item.path, '')); return }
    openFile(item)
  }

  return { navigateToDir, toggleShowHidden, clickItem, viewItem }
}
