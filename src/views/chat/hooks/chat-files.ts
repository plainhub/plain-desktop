import { computed, ref } from 'vue'
import { useRouter } from 'vue-router'
import { storeToRefs } from 'pinia'
import { getFileName, getFileUrl, notId, getFileExtension, getPeerProxyUrl, getFileId } from '@/lib/api/file'
import type { ISource } from '@/components/lightbox/types'
import { isVideo, isImage, isAudio, isTextFile, canOpenInBrowser, isAppFile } from '@/lib/file'
import { useTempStore } from '@/stores/temp'
import { useMainStore } from '@/stores/main'
import { openUrl } from '@/lib/browser'
import { openWindow } from '@/lib/api/tauri-window'
import { isLocalMode } from '@/lib/device/local-mode'
import { useOpenMedia } from '@/hooks/open-media'
import { useRevealFile } from './reveal-file'

export function useChatFiles(props: { data: any; downloadInfo: any; peer: { ip: string; port: number } | null }) {
  const tempStore = useTempStore()
  const mainStore = useMainStore()
  const router = useRouter()
  const { urlTokenKey } = storeToRefs(tempStore)

  const activeAudioSrc = ref<string | null>(null)
  const iconErrors = ref<string[]>([])

  const { open: openMedia } = useOpenMedia()
  const { revealFile } = useRevealFile()

  const items = computed<ISource[]>(() => {
    const files = props.data?._content?.value?.items ?? []
    const peer = props.peer
    return (props.data?.data?.ids ?? []).map((id: string, i: number) => {
      const f = files[i]
      const uri = f.uri
      const peerFileId = typeof uri === 'string' && uri.startsWith('fsid:') ? uri.slice(4) : ''
      const isGif = typeof uri === 'string' && uri.endsWith('.gif')
      // Received from a peer (`fsid:`): route through /proxyfs with the
      // resize/conversion query baked in (see ChatImages.vue for details).
      const src = peer && peerFileId
        ? getPeerProxyUrl(tempStore.urlTokenKey, peer, peerFileId, isGif ? '' : '&w=1024&h=1024&cc=false')
        : getFileUrl(id)
      return {
        path: f.uri, src,
        viewOriginImage: notId(id) || isGif,
        name: getFileName(f.fileName ?? f.uri), duration: f.duration, size: f.size,
        fileId: id, thumbnail: f.thumbnail, extension: getFileExtension(f.uri),
        summary: f.summary || undefined, isFromChat: true,
      }
    })
  })

  const ACTIVE_STATUSES = ['PENDING', 'DOWNLOADING', 'PAUSED', 'FAILED']
  const isActiveDl = computed(() => !!props.downloadInfo && ACTIVE_STATUSES.includes(props.downloadInfo.status))

  function getThumb(item: ISource) {
    if (isImage(item.name) || isVideo(item.name)) {
      if (item.thumbnail) return item.thumbnail
      // Received from a peer: `fsid:` URIs live on the peer — route through
      // /proxyfs (the local /fs cannot resolve `fsid:`).
      if (props.peer && item.path.startsWith('fsid:')) {
        return getPeerProxyUrl(tempStore.urlTokenKey, props.peer, item.path.slice(4), '&w=50&h=50')
      }
      if (isActiveDl.value) {
        return ''
      }
      return item.src.startsWith('blob:') ? item.src : `${item.src}&w=50&h=50`
    }
    const ext = item.extension
    if (ext && !iconErrors.value.includes(item.name)) return `/ficons/${ext}.svg`
    return '/ficons/default.svg'
  }

  function onIconError(name: string) {
    if (!iconErrors.value.includes(name)) iconErrors.value.push(name)
  }

  function clickItem(item: ISource) {
    if (isAudio(item.name)) {
      activeAudioSrc.value = activeAudioSrc.value === item.src ? null : item.src
      return
    }
    if (isTextFile(item.name) && item.fileId) {
      let textFileId = item.fileId
      if (isAppFile(item.path) && urlTokenKey.value) {
        textFileId = getFileId(urlTokenKey.value, JSON.stringify({ path: item.path, name: item.name }))
      }
      const path = `/text-file?id=${encodeURIComponent(textFileId)}`
      if (__IS_TAURI__) {
        openWindow(path)
        return
      }
      openUrl(path)
    } else if (canOpenInBrowser(item.name)) {
      openUrl(item.src)
    } else if (isImage(item.name) || isVideo(item.name)) {
      const viewable = items.value.filter((it) => isImage(it.name) || isVideo(it.name))
      const index = viewable.findIndex((it) => it.src === item.src)
      if (index < 0) {
        openMedia(0, [item])
        return
      }
      openMedia(index, viewable)
    } else {
      // Unopenable files (zip, apk, …): in local mode the file is on this
      // machine — reveal it instead of downloading it again.
      if (__IS_TAURI__ && isLocalMode() && item.path && !item.path.startsWith('fsid:')) {
        revealFile(item.path)
        return
      }
      openUrl(item.src)
    }
  }

  return { items, activeAudioSrc, getThumb, onIconError, clickItem }
}
