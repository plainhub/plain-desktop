import { chatAttachmentThumbnail } from '@/lib/api/file-action'
import { isLocalMode } from '@/lib/device/local-mode'
import { computed, ref } from 'vue'
import { storeToRefs } from 'pinia'
import { getFileName, getFileUrl, getFileExtension, getChatAttachmentUrl, getChatFileId } from '@/lib/api/file'
import { getTextFileId } from '@/lib/api/text-file'
import type { ISource } from '@/components/lightbox/types'
import { isVideo, isImage, isAudio, isTextFile, canOpenInBrowser, isAppFile } from '@/lib/file'
import { useTempStore } from '@/stores/temp'
import { openUrl } from '@/lib/browser'
import { useOpenMedia } from '@/hooks/open-media'
import { useRevealFile } from './reveal-file'
import { chatFileDurationMs } from '../chat-file-duration'

export function useChatFiles(props: { data: any; downloadInfo: any; peer: { id: string } | null }) {
  const tempStore = useTempStore()
  const { urlTokenKey } = storeToRefs(tempStore)

  const activeAudioSrc = ref<string | null>(null)
  const iconErrors = ref<string[]>([])

  const { open: openMedia } = useOpenMedia()
  const { revealFile } = useRevealFile()

  const items = computed<ISource[]>(() => {
    const data = props.data
    const files = data?.content?.items ?? []
    const peer = props.peer
    return files.map((f: any) => {
      const uri = f.uri
      const remote = typeof uri === 'string' && uri.startsWith('fsid:')
      const isGif = typeof uri === 'string' && uri.endsWith('.gif')
      const fileId = remote ? '' : getChatFileId(urlTokenKey.value, uri)
      const src = peer && remote
        ? getChatAttachmentUrl(urlTokenKey.value, peer.id, uri, isGif ? '' : '&size=lg')
        : getFileUrl(fileId)
      return {
        path: f.uri, src,
        viewOriginImage: isGif,
        name: getFileName(f.fileName ?? f.uri), durationMs: chatFileDurationMs(f), size: f.size,
        fileId, thumbnail: f.thumbnail, extension: getFileExtension(f.uri),
        summary: f.summary || undefined, isFromChat: true,
      }
    })
  })

  function getThumb(item: ISource) {
    if (isImage(item.name) || isVideo(item.name)) {
      if (item.thumbnail) return item.thumbnail
      if (item.path.startsWith('fsid:')) return chatAttachmentThumbnail(item.src)
      return item.src.startsWith('blob:') ? item.src : `${item.src}&size=sm`
    }
    const ext = item.extension
    if (ext && !iconErrors.value.includes(item.name)) return `/ficons/${ext}.svg`
    return '/ficons/default.svg'
  }

  function onIconError(name: string) {
    if (!iconErrors.value.includes(name)) iconErrors.value.push(name)
  }

  function clickItem(item: ISource) {
    if (item.path.startsWith('fsid:')) return
    if (isAudio(item.name)) {
      activeAudioSrc.value = activeAudioSrc.value === item.src ? null : item.src
      return
    }
    if (isTextFile(item.name) && item.fileId) {
      let textFileId = item.fileId
      if (isAppFile(item.path) && urlTokenKey.value) {
        textFileId = getTextFileId(urlTokenKey.value, item.path)
      }
      const path = `/text-file?id=${encodeURIComponent(textFileId)}`
      openUrl(path)
    } else if (canOpenInBrowser(item.name)) {
      openUrl(item.src)
    } else if (isImage(item.name) || isVideo(item.name)) {
      const viewable = items.value.filter((it) => !it.path.startsWith('fsid:') && (isImage(it.name) || isVideo(it.name)))
      const index = viewable.findIndex((it) => it.src === item.src)
      if (index < 0) {
        openMedia(0, [item])
        return
      }
      openMedia(index, viewable)
    } else {
      // Unopenable files (zip, apk, …): in local mode the file is on this
      // machine — reveal it instead of downloading it again.
      if (isLocalMode() && item.path && !item.path.startsWith('fsid:')) {
        revealFile(item.path)
        return
      }
      openUrl(item.src)
    }
  }

  return { items, activeAudioSrc, getThumb, onIconError, clickItem }
}
