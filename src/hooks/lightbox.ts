import { computed, ref, reactive, watch, onMounted, onBeforeUnmount, type Ref } from 'vue'
import { on, off, isArray } from '@/components/lightbox/utils/index'
import { useImage, useMouse, useTouch } from '@/components/lightbox/utils/hooks'
import type { ISource, IImgWrapperState, IndexChangeActions } from '@/components/lightbox/types'
import { isVideo, isImage, isAudio, isSvg, isHeic, fileNameError } from '@/lib/file'
import { browserSupportsHevc, ensurePlayableVideoUrl } from '@/lib/video-codec'
import { resolveFileUrlByPath } from '@/lib/api/file'
import { createNativeResourceOwner, isNativeResourceUrl, type NativeResourceOwner } from '@/hooks/native-resource-url'
import { useTempStore } from '@/stores/temp'
import { storeToRefs } from 'pinia'
import { useMainStore } from '@/stores/main'
import { fileInfoGQL, initLazyQuery, tagRelationsGQL, tagsGQL } from '@/lib/api/query'
import { openModal } from '@/components/modal'
import { useI18n } from 'vue-i18n'
import type { IItemTagsUpdatedEvent, IFileDeletedEvent, IFileRenamedEvent, ITag, IMediaItemsActionedEvent } from '@/lib/interfaces'
import emitter from '@/plugins/eventbus'
import { useMediaPlayHistory } from './media-play-history'
import { useRename } from '@/hooks/files'
import { download, getFileName } from '@/lib/api/file'
import { useDeleteItems } from '@/hooks/media'
import { DataType } from '@/lib/data'
import { arrayRemove } from '@/lib/array'
import DeleteFileConfirm from '@/components/DeleteFileConfirm.vue'
import EditValueModal from '@/components/EditValueModal.vue'

export function useLightboxState(isPhone: boolean) {
  const tempStore = useTempStore()
  const { urlTokenKey, app } = storeToRefs(tempStore)
  const { lightboxInfoVisible, imageViewQuality } = storeToRefs(useMainStore())

  const { imgRef, imgState, setImgSize } = useImage()
  const imgIndex = ref(0)
  const current = ref<ISource>()
  const fileInfo = ref<any>(null)
  const video = ref<HTMLVideoElement>()

  const imgWrapperState = reactive<IImgWrapperState>({
    scale: 1,
    lastScale: 1,
    rotateDeg: 0,
    top: 0,
    left: 0,
    initX: 0,
    initY: 0,
    lastX: 0,
    lastY: 0,
    touches: [] as TouchList | [],
  })

  const status = reactive({
    loadError: false,
    loading: false,
    dragging: false,
    gesturing: false,
    swipeToLeft: false,
    swipeToRight: false,
    wheeling: false,
  })

  const imgWrapperStyle = computed(() => {
    const mobileOffset = isPhone ? -28 : 0
    return {
      cursor: status.loadError ? 'default' : 'move',
      top: `calc(50% + ${imgWrapperState.top + mobileOffset}px)`,
      left: `calc(50% + ${imgWrapperState.left}px)`,
      transition: status.dragging || status.gesturing ? 'none' : '',
      transform: `translate(-50%, -50%) scale(${imgWrapperState.scale}) rotate(${imgWrapperState.rotateDeg}deg)`,
    }
  })

  return {
    tempStore,
    urlTokenKey,
    app,
    lightboxInfoVisible,
    imageViewQuality,
    imgRef,
    imgState,
    setImgSize,
    imgIndex,
    current,
    fileInfo,
    video,
    imgWrapperState,
    status,
    imgWrapperStyle,
  }
}

export function useLightboxQueries(
  current: Ref<ISource | undefined>,
  fileInfo: Ref<any>,
  imgState: { naturalWidth: number; naturalHeight: number },
) {
  function updateViewOriginImageState() {
    if (current.value && fileInfo.value && isImage(current.value.name) && current.value.path === fileInfo.value.path) {
      if (isSvg(current.value.name)) {
        current.value.viewOriginImage = true
      } else if (isHeic(current.value.name)) {
        // HEIC files must always go through server-side conversion; raw HEIC is not displayable in most browsers
      } else {
        const { width, height } = fileInfo.value.data
        if (width === imgState.naturalWidth && height === imgState.naturalHeight) {
          current.value.viewOriginImage = true
        }
      }
    }
  }

  const {
    fetch: loadInfo,
  } = initLazyQuery({
    handle: (data: any, error: string) => {
      if (!error && data) {
        fileInfo.value = data.fileInfo
        updateViewOriginImageState()
      }
    },
    document: fileInfoGQL,
    variables: () => ({
      path: current.value?.path ?? '',
      fileName: current.value?.name,
    }),
  })

  // The file's own applied tags — lazy-loaded via tagRelations and joined
  // against tagsMap, instead of being shipped inside fileInfo.
  const itemTags = ref<{ tagId: string; key: string }[]>([])
  const { fetch: loadItemTags } = initLazyQuery({
    handle: (data: any, error: string) => {
      if (!error && data) {
        itemTags.value = data.tagRelations ?? []
      }
    },
    document: tagRelationsGQL,
    variables: () => ({
      type: current.value?.type ?? '',
      keys: [current.value?.data?.id ?? ''],
    }),
  })

  const tagsMap = new Map<string, ITag[]>()
  const { fetch: loadTags } = initLazyQuery({
    handle: (data: any, _error: string) => {
      if (data) {
        tagsMap.set(current.value?.type ?? '', data.tags)
      }
    },
    document: tagsGQL,
    variables: () => ({
      type: current.value?.type ?? '',
    }),
  })

  return { loadInfo, refetchInfo: () => { loadInfo(); loadItemTags() }, updateViewOriginImageState, tagsMap, loadTags, itemTags, loadItemTags }
}

export function useLightboxTransform(
  imgWrapperState: IImgWrapperState,
  imgState: { maxScale: number },
  status: { loadError: boolean; loading: boolean; dragging: boolean; gesturing: boolean; wheeling: boolean },
) {
  const defaultScale = 1.5

  const zoom = (newScale: number) => {
    if (Math.abs(1 - newScale) < 0.05) newScale = 1
    else if (Math.abs(imgState.maxScale - newScale) < 0.05) newScale = imgState.maxScale
    imgWrapperState.lastScale = imgWrapperState.scale
    imgWrapperState.scale = newScale
  }

  const zoomIn = () => {
    const s = imgWrapperState.scale * defaultScale
    if (s < imgState.maxScale * 100) zoom(s)
  }

  const zoomOut = () => {
    const s = imgWrapperState.scale / defaultScale
    if (s > 0.1) zoom(s)
  }

  const rotateLeft = () => { imgWrapperState.rotateDeg -= 90 }
  const rotateRight = () => { imgWrapperState.rotateDeg += 90 }

  const resize = () => {
    imgWrapperState.scale = 1
    imgWrapperState.top = 0
    imgWrapperState.left = 0
  }

  const onDblclick = () => {
    if (imgWrapperState.scale !== imgState.maxScale) {
      imgWrapperState.lastScale = imgWrapperState.scale
      imgWrapperState.scale = imgState.maxScale
    } else {
      imgWrapperState.scale = imgWrapperState.lastScale
    }
  }

  const onWheel = (e: WheelEvent) => {
    if (status.loadError || status.gesturing || status.loading || status.dragging || status.wheeling) return
    status.wheeling = true
    setTimeout(() => { status.wheeling = false }, 80)
    if (e.deltaY < 0) zoomIn()
    else zoomOut()
  }

  return { zoomIn, zoomOut, rotateLeft, rotateRight, resize, onDblclick, onWheel }
}

// The single source of truth for which URL a source displays at, given the quality
// setting. Shared by the preloader and the visible <img> so they always agree.
export function getImageDisplayUrl(source: ISource, quality: 'fast' | 'original'): string {
  // A native resource URL serves the original file; the server-side
  // thumbnail query has no native equivalent.
  if (isNativeResourceUrl(source.src)) {
    return source.src
  }
  const useOriginal = !isHeic(source.name) && (isSvg(source.name) || source.viewOriginImage || quality === 'original')
  return useOriginal ? source.src : source.src + '&size=lg'
}

export function useLightboxNavigation(
  tempStore: ReturnType<typeof useTempStore>,
  imgIndex: Ref<number>,
  current: Ref<ISource | undefined>,
  imgWrapperState: IImgWrapperState,
  status: { loadError: boolean; loading: boolean; dragging: boolean; gesturing: boolean },
  tagsMap: Map<string, ITag[]>,
  loadTags: () => void,
  loadInfo: () => void,
  loadItemTags: () => void,
  loop: Ref<boolean>,
  emit: (event: string, ...args: any[]) => void,
  imageViewQuality: Ref<'fast' | 'original'>,
) {
  const reset = (keepImageVisible = false) => {
    imgWrapperState.scale = 1
    imgWrapperState.lastScale = 1
    imgWrapperState.rotateDeg = 0
    imgWrapperState.top = 0
    imgWrapperState.left = 0
    status.loadError = false
    status.dragging = false
    status.gesturing = false
    status.loading = !keepImageVisible
  }

  let generation = 0
  let stopPreloadWatch: (() => void) | undefined
  const bindings = new Map<ISource, { owner: NativeResourceOwner; promise: Promise<string>; assigned?: string }>()
  const decodedUrls = new Set<string>()
  const readyVideoUrls = new Set<string>()
  const preloads = new Map<string, () => void>()

  const release = (source: ISource) => {
    const binding = bindings.get(source)
    if (!binding) return
    bindings.delete(source)
    void binding.owner.dispose()
    if (binding.assigned && isNativeResourceUrl(binding.assigned)) {
      if (source.src === binding.assigned) source.src = ''
      if (source.playbackUrl === binding.assigned) {
        source.playbackUrl = undefined
        source.transcoded = undefined
      }
    }
  }

  const trimPreloads = () => {
    const retained = new Set([...bindings.keys()].map(source => isImage(source.name)
      ? getImageDisplayUrl(source, imageViewQuality.value) : source.src))
    for (const [url, dispose] of preloads) {
      if (retained.has(url)) continue
      preloads.delete(url)
      decodedUrls.delete(url)
      readyVideoUrls.delete(url)
      dispose()
    }
  }

  const retainSources = (sources: Set<ISource>) => {
    for (const source of bindings.keys()) if (!sources.has(source)) release(source)
    trimPreloads()
  }

  const clearResources = () => {
    generation++
    stopPreloadWatch?.()
    stopPreloadWatch = undefined
    retainSources(new Set())
    current.value = undefined
  }

  const closeDialog = () => {
    clearResources()
    tempStore.lightbox.visible = false
    tempStore.lightbox.index = -1
    imgIndex.value = 0
  }

  const resolveSource = (source: ISource) => {
    const existing = bindings.get(source)
    if (existing) return existing.promise
    const owner = createNativeResourceOwner()
    const binding = { owner, promise: Promise.resolve(''), assigned: undefined as string | undefined }
    bindings.set(source, binding)
    binding.promise = (async () => {
      if (source.src) {
        if (isNativeResourceUrl(source.src)) owner.retain(source.src)
        return source.src
      }
      const url = await resolveFileUrlByPath(owner, tempStore.urlTokenKey, source.path, source.size, source.updatedAt ? '&v=' + encodeURIComponent(source.updatedAt) : '')
      if (bindings.get(source) !== binding) throw new Error('resource_closed')
      source.src = url
      binding.assigned = url
      return url
    })().catch(error => {
      if (bindings.get(source) === binding) {
        release(source)
        trimPreloads()
        if (tempStore.lightbox.visible) console.warn('Lightbox resource failed', error)
      }
      throw error
    })
    return binding.promise
  }

  const preloadSource = async (source: ISource) => {
    const pending = resolveSource(source)
    try {
      await pending
      if (!bindings.has(source) || !tempStore.lightbox.visible || !source.src) return
      const url = isImage(source.name) ? getImageDisplayUrl(source, imageViewQuality.value) : source.src
      if (preloads.has(url)) return
      if (isImage(source.name)) {
        const img = new Image()
        const dispose = () => { img.onload = null; img.onerror = null; img.removeAttribute('src') }
        preloads.set(url, dispose)
        img.onload = () => {
          const markDecoded = () => { if (preloads.get(url) === dispose) decodedUrls.add(url) }
          if (img.decode) img.decode().then(markDecoded).catch(markDecoded)
          else markDecoded()
        }
        img.src = url
      } else if (isVideo(source.name)) {
        const videoEl = document.createElement('video')
        const ready = () => { if (preloads.has(url)) readyVideoUrls.add(url) }
        preloads.set(url, () => {
          videoEl.removeEventListener('canplay', ready)
          videoEl.removeAttribute('src')
          videoEl.load()
        })
        videoEl.preload = 'auto'
        videoEl.muted = true
        videoEl.addEventListener('canplay', ready)
        videoEl.src = url
        videoEl.load()
      }
    } catch (error) {
      if (bindings.get(source)?.promise === pending && tempStore.lightbox.visible) console.warn('Lightbox preload failed', error)
    }
  }

  const neighbours = (sources: ISource[], index: number) => {
    const selected = new Set<ISource>()
    if (sources.length < 2) return selected
    for (const offset of [1, 2, -1]) {
      let next = index + offset
      if (loop.value) next = ((next % sources.length) + sources.length) % sources.length
      const source = sources[next]
      if (!source || next === index) continue
      if (isImage(source.name) || (isVideo(source.name) && browserSupportsHevc())) selected.add(source)
    }
    return selected
  }

  const changeIndex = async (newIndex: number, actions?: IndexChangeActions) => {
    const sources = tempStore.lightbox.sources
    const source = sources[newIndex]
    if (!source || !tempStore.lightbox.visible) return
    const change = ++generation
    stopPreloadWatch?.()
    stopPreloadWatch = undefined
    const active = () => change === generation && tempStore.lightbox.visible && tempStore.lightbox.sources === sources
    const oldIndex = imgIndex.value
    const adjacent = neighbours(sources, newIndex)
    const wanted = new Set([source, ...adjacent])
    if (current.value) wanted.add(current.value)
    retainSources(wanted)
    try {
      await resolveSource(source)
      if (!active()) return
      if (isVideo(source.name) && source.src && !source.playbackUrl) {
        const { url, transcoded } = await ensurePlayableVideoUrl(source.src, source.isFromChat)
        if (!active()) return
        source.playbackUrl = url
        source.transcoded = transcoded
      }
      const alreadyDecoded = (isImage(source.name) && decodedUrls.has(getImageDisplayUrl(source, imageViewQuality.value))) ||
        (isVideo(source.name) && readyVideoUrls.has(source.src))
      reset(alreadyDecoded)
      imgIndex.value = newIndex
      current.value = source
      setTimeout(() => {
        if (!active()) return
        const type = source.type ?? ''
        if (type && !tagsMap.has(type)) loadTags()
        if (!tempStore.lightbox.readOnly) { loadInfo(); loadItemTags() }
      }, 0)
      const preload = () => {
        if (!active()) return
        retainSources(new Set([source, ...adjacent]))
        for (const neighbour of adjacent) void preloadSource(neighbour)
      }
      if (isVideo(source.name) && status.loading) {
        retainSources(new Set([source]))
        stopPreloadWatch = watch(() => status.loading, loading => {
          if (loading) return
          stopPreloadWatch?.()
          stopPreloadWatch = undefined
          preload()
        })
      } else preload()
      if (oldIndex !== newIndex) {
        if (actions) {
          if (isArray(actions)) actions.forEach(action => emit(action, oldIndex, newIndex))
          else emit(actions, oldIndex, newIndex)
        }
        emit('on-index-change', oldIndex, newIndex)
      }
    } catch (error) {
      if (!active()) return
      release(source)
      trimPreloads()
      status.loading = false
      status.loadError = true
      emit('on-error', error)
    }
  }

  watch(() => [tempStore.lightbox.sources, tempStore.lightbox.index, tempStore.lightbox.visible] as const,
    ([sources, index, visible], previous) => {
      if (!visible) { clearResources(); return }
      if (previous && sources !== previous[0]) clearResources()
      if (index >= 0 && index < sources.length) void changeIndex(index)
    }, { immediate: true })
  onBeforeUnmount(clearResources)

  const onNext = () => {
    const total = tempStore.lightbox.sources.length
    if (total == 1) {
      return
    }
    const oldIndex = imgIndex.value
    const newIndex = loop.value ? (oldIndex + 1) % tempStore.lightbox.sources.length : oldIndex + 1
    if (!loop.value && newIndex > total - 1) return
    changeIndex(newIndex, ['on-next', 'on-next-click'])
  }

  const onPrev = () => {
    const total = tempStore.lightbox.sources.length
    if (total == 1) {
      return
    }
    const oldIndex = imgIndex.value
    let newIndex = oldIndex - 1
    if (oldIndex === 0) {
      if (!loop.value) return
      newIndex = total - 1
    }
    changeIndex(newIndex, ['on-prev', 'on-prev-click'])
  }

  return { reset, closeDialog, changeIndex, onNext, onPrev }
}

export function useLightboxFileActions(
  current: Ref<ISource | undefined>,
  refetchInfo: () => void,
  isPhone: boolean,
  lightboxInfoVisible: Ref<boolean>,
) {
  const { t } = useI18n()
  const { deleteItem } = useDeleteItems()
  const { renameItem, renamedPath, renameDone, renameMutation, renameVariables } = useRename(() => {
    refetchInfo()
  })

  function downloadFile() {
    const source = current.value
    if (!source) return
    download(source.src, (source.name || getFileName(source.path)).replace(' ', '-'), source.isFromChat, source.path)
  }

  function deleteFile() {
    const mediaTypes = [DataType.VIDEO, DataType.AUDIO, DataType.IMAGE]
    const type = current.value?.type
    const item = current.value?.data
    if (type && mediaTypes.includes(type)) {
      deleteItem(type, item)
    } else {
      openModal(DeleteFileConfirm, {
        files: [item],
        onError: refetchInfo,
        onDone: () => {
          emitter.emit('file_deleted', { item })
        },
      })
    }
  }

  function renameFile() {
    const item = current.value?.data
    if (!item || !current.value?.path) return

    renameItem.value = {
      id: item.id,
      path: current.value.path,
      name: getFileName(current.value.path),
      size: current.value.size || 0,
      isDir: false,
      isSystem: false,
      extension: '',
      fileId: '',
      updatedAt: '',
      createdAt: '',
    }

    openModal(EditValueModal, {
      title: t('rename'),
      placeholder: t('name'),
      value: getFileName(current.value.path),
      validate: fileNameError,
      mutation: renameMutation,
      getVariables: renameVariables,
      done: (newName: string) => {
        if (current.value && renamedPath.value) {
          const oldPath = current.value.path
          const newPath = renamedPath.value
          emitter.emit('file_renamed', { oldPath, newPath, item: { ...current.value.data, path: newPath, name: newName } })
          current.value.path = newPath
          current.value.name = newName
        }
        return renameDone(newName)
      },
    })
  }

  function handleActionSuccess(action: string) {
    if (isPhone && (action === 'trash' || action === 'restore')) {
      lightboxInfoVisible.value = false
    }
  }

  return { downloadFile, deleteFile, renameFile, handleActionSuccess }
}

export function useLightboxEvents(
  tempStore: ReturnType<typeof useTempStore>,
  current: Ref<ISource | undefined>,
  video: Ref<HTMLVideoElement | undefined>,
  status: { loading: boolean; loadError: boolean },
  imgState: { maxScale: number },
  imgIndex: Ref<number>,
  setImgSize: () => void,
  refetchInfo: () => void,
  updateViewOriginImageState: () => void,
  closeDialog: () => void,
  onNext: () => void,
  onPrev: () => void,
  changeIndex: (i: number) => void,
  emit: (event: string, ...args: any[]) => void,
  popup: Ref<boolean> = ref(false),
) {
  let isVideoPlaying = true
  let videoLoad = 0
  let releaseVideo: (() => void) | undefined
  const history = useMediaPlayHistory()

  const onVideoLoaded = async (event: Event) => {
    const element = event.currentTarget as HTMLVideoElement
    const source = current.value
    if (!source) return
    const load = ++videoLoad
    releaseVideo?.()
    releaseVideo = undefined
    const active = () => load === videoLoad && tempStore.lightbox.visible && current.value === source && video.value === element
    const mediaId = source.type === DataType.VIDEO ? source.data?.id : undefined
    try {
      const release = await history.restoreAndPlay(element, mediaId ? { mediaId, type: 'VIDEO' } : undefined, active)
      if (active()) releaseVideo = release
      else release?.()
    } catch (error) {
      if (!active()) return
      status.loading = false
      status.loadError = true
      emitter.emit('toast', error instanceof Error ? error.message : 'network_error')
    }
  }

  watch(() => tempStore.lightbox.visible, visible => {
    if (!visible) {
      videoLoad++
      releaseVideo?.()
      releaseVideo = undefined
    }
  })

  const onLoad = () => {
    status.loading = false
    if (current.value && isImage(current.value.name)) {
      setImgSize()
      updateViewOriginImageState()
    }
  }

  const onError = (e: Event) => {
    status.loading = false
    status.loadError = true
    emit('on-error', e)
  }

  const onPlaying = () => { status.loading = false; isVideoPlaying = true; video.value?.blur() }
  const onPause = () => { isVideoPlaying = false; video.value?.blur() }
  const onVolumeChange = () => { video.value?.blur() }

  const onKeyPress = (e: Event) => {
    if (!tempStore.lightbox.visible) return
    const evt = e as KeyboardEvent
    if (evt.key === 'Escape') {
      if (popup.value) return
      if (document.querySelector('.vue-modal')) return
      evt.stopPropagation()
      closeDialog()
    } else if (evt.key === 'ArrowLeft') {
      evt.stopPropagation()
      onPrev()
    } else if (evt.key === 'ArrowRight') {
      evt.stopPropagation()
      onNext()
    } else if (evt.key === ' ') {
      const v = video.value
      if (v) {
        if (v.paused && !isVideoPlaying) v.play()
        else v.pause()
      }
    }
  }

  const onWindowResize = () => { setImgSize() }

  // Event bus handlers
  const itemTagsUpdatedHandler = (event: IItemTagsUpdatedEvent) => {
    if (event.item.key === current.value?.data?.id) refetchInfo()
  }

  const navigateAfterRemove = () => {
    if (tempStore.lightbox.sources.length === 0) {
      closeDialog()
    } else {
      const newIndex = Math.min(imgIndex.value, tempStore.lightbox.sources.length - 1)
      changeIndex(newIndex)
    }
  }

  const mediaItemsActionedHandler = (event: IMediaItemsActionedEvent) => {
    const query = `ids:${current.value?.data?.id}`
    if (['delete', 'trash', 'restore'].includes(event.action) && event.query === query) {
      arrayRemove(tempStore.lightbox.sources, (it: ISource) => `ids:${it.data?.id}` === event.query)
      navigateAfterRemove()
    }
  }

  const fileDeletedHandler = (event: IFileDeletedEvent) => {
    if (event.item.path === current.value?.data?.path) {
      arrayRemove(tempStore.lightbox.sources, (it: ISource) => it.path === event.item.path)
      navigateAfterRemove()
    }
  }

  const fileRenamedHandler = (event: IFileRenamedEvent) => {
    tempStore.lightbox.sources.forEach((source: ISource) => {
      if (source.path === event.oldPath) {
        source.path = event.newPath
        source.name = getFileName(event.newPath)
        if (source.data) {
          source.data.path = event.newPath
          source.data.name = getFileName(event.newPath)
        }
      }
    })
    if (current.value && current.value.path === event.oldPath) {
      current.value.path = event.newPath
      current.value.name = getFileName(event.newPath)
      if (current.value.data) {
        current.value.data.path = event.newPath
        current.value.data.name = getFileName(event.newPath)
      }
    }
  }

  // Lifecycle
  onMounted(() => {
    on(window, 'keydown', onKeyPress)
    on(window, 'resize', onWindowResize)
    emitter.on('item_tags_updated', itemTagsUpdatedHandler)
    emitter.on('media_items_actioned', mediaItemsActionedHandler)
    emitter.on('file_deleted', fileDeletedHandler)
    emitter.on('file_renamed', fileRenamedHandler)
  })

  onBeforeUnmount(() => {
    videoLoad++
    releaseVideo?.()
    off(window, 'keydown', onKeyPress)
    off(window, 'resize', onWindowResize)
    emitter.off('item_tags_updated', itemTagsUpdatedHandler)
    emitter.off('media_items_actioned', mediaItemsActionedHandler)
    emitter.off('file_deleted', fileDeletedHandler)
    emitter.off('file_renamed', fileRenamedHandler)
  })

  return { onLoad, onVideoLoaded, onError, onPlaying, onPause, onVolumeChange }
}

export function useLightboxMouseTouch(
  imgWrapperState: IImgWrapperState,
  imgState: { width: number; height: number; naturalWidth: number; naturalHeight: number; maxScale: number },
  status: { loadError: boolean; loading: boolean; dragging: boolean; gesturing: boolean; swipeToLeft: boolean; swipeToRight: boolean },
) {
  const canMove = (button?: number) => button === 0
  const { onMouseDown, onMouseMove, onMouseUp } = useMouse(imgWrapperState, status, canMove)
  const { onTouchStart, onTouchMove, onTouchEnd } = useTouch(imgState, imgWrapperState, status, canMove)
  return { onMouseDown, onMouseMove, onMouseUp, onTouchStart, onTouchMove, onTouchEnd }
}
