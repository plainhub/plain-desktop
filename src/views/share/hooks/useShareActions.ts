import { ref, type Ref } from 'vue'
import { download, getFileExtension } from '@/lib/api/file'
import { canView } from '@/lib/file'
import type { ISource } from '@/components/lightbox/types'
import { useOpenMedia } from '@/hooks/open-media'
import { useFileOpen } from '@/hooks/file-open'
import { openUrl } from '@/lib/browser'
import { getSharedDirUrl, getSharedFileId, getSharedFileUrl, type SharedFile, type SharedInfo } from '@/lib/api/guest'

export function useShareActions(
  sharedId: Ref<string>,
  info: Ref<SharedInfo | null>,
  entries: Ref<SharedFile[]>,
  navigateTo: (path: string) => void,
) {
  const thumbErrorIds = ref<string[]>([])
  const extErrorIds = ref<string[]>([])

  const { open: openMedia } = useOpenMedia()

  function fileUrl(entry: SharedFile, query: string = ''): string {
    return getSharedFileUrl(info.value!.urlToken, sharedId.value, entry.virtualPath, query)
  }

  function entryExt(entry: SharedFile): string {
    return getFileExtension(entry.name)
  }

  function thumbSrc(entry: SharedFile): string {
    return getSharedFileUrl(info.value!.urlToken, sharedId.value, entry.virtualPath, '&w=96&h=96')
  }

  function onThumbError(virtualPath: string) {
    thumbErrorIds.value.push(virtualPath)
  }

  function onExtError(ext: string) {
    extErrorIds.value.push(ext)
  }

  function toSource(entry: SharedFile): ISource {
    return { src: fileUrl(entry), path: entry.virtualPath, name: entry.name, size: entry.size, durationMs: 0 }
  }

  function downloadEntry(entry: SharedFile) {
    const url = entry.isDir
      ? getSharedDirUrl(info.value!.urlToken, sharedId.value, entry.virtualPath)
      : fileUrl(entry, '&dl=1')
    download(url, entry.isDir ? `${entry.name}.zip` : entry.name)
  }

  const { openFile } = useFileOpen<SharedFile>({
    items: entries,
    openTextFile: (entry) => {
      const fileId = getSharedFileId(info.value!.urlToken, sharedId.value, entry.virtualPath)
      openUrl(`/text-file?id=${encodeURIComponent(fileId)}&sid=${encodeURIComponent(sharedId.value)}`)
    },
    openBrowserFile: (entry) => window.open(fileUrl(entry), '_blank', 'noopener'),
    viewMedia: (list, f) => {
      const media = list.filter((it) => !it.isDir && canView(it.name)).map(toSource)
      openMedia(Math.max(0, media.findIndex((s) => s.path === f.virtualPath)), media, true)
    },
    download: downloadEntry,
  })

  function onItemClick(entry: SharedFile) {
    if (entry.isDir) navigateTo(entry.virtualPath)
    else openFile(entry)
  }

  return {
    thumbErrorIds,
    extErrorIds,
    entryExt,
    thumbSrc,
    onThumbError,
    onExtError,
    downloadEntry,
    onItemClick,
  }
}
