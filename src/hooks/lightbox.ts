import { showActionError } from '@/hooks/show-action-error'
import { HttpErrorCode } from '@/lib/api/http-error-code'
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
import { useDeleteMediaItem } from '@/hooks/media'
import { DataType } from '@/lib/data'
import { arrayRemove } from '@/lib/array'
import { deleteFilesGQL, initMutation } from '@/lib/api/mutation'
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
) {
  const {
    fetch: loadInfo,
  } = initLazyQuery({
    handle: (data: any, error: string) => {
      if (!error && data) {
        fileInfo.value = data.fileInfo
      }
    },
    document: fileInfoGQL,
    variables: () => ({
      path: current.value?.path ?? '',
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

  return { loadInfo, refetchInfo: () => { loadInfo(); loadItemTags() }, tagsMap, loadTags, itemTags, loadItemTags }
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

function usesImagePreview(source: ISource, quality: 'fast' | 'original') {
  return isHeic(source.name) || (quality === 'fast' && !isSvg(source.name) && !source.viewOriginImage)
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

  const history = useMediaPlayHistory()
  const videoLoad = ref<{ key: number; loaded: (event: Event) => void }>()
  let retireVideo: (() => void) | undefined
  function stopVideo() {
    retireVideo?.()
    retireVideo = undefined
    videoLoad.value = undefined
  }
  let generation = 0
  let stopPreloadWatch: (() => void) | undefined
  const bindings = new Map<ISource, { owner: NativeResourceOwner; promise: Promise<string>; assigned?: string; preview?: string }>()
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

  const displayUrl = (source: ISource) => {
    if (!isImage(source.name) || !usesImagePreview(source, imageViewQuality.value)) return source.src
    if (isNativeResourceUrl(source.src)) return bindings.get(source)?.preview
    return source.src + '&size=LG'
  }

  const resolveDisplayUrl = async (source: ISource) => {
    const url = displayUrl(source)
    if (url !== undefined) return url
    const binding = bindings.get(source)!
    const version = source.updatedAt ? '&v=' + encodeURIComponent(source.updatedAt) : ''
    binding.preview = await resolveFileUrlByPath(binding.owner, tempStore.urlTokenKey, source.path, source.size, '&size=LG' + version)
    return binding.preview
  }

  const trimPreloads = () => {
    const retained = new Set([...bindings.keys()].map(displayUrl))
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
    stopVideo()
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
      const url = await resolveDisplayUrl(source)
      if (bindings.get(source)?.promise !== pending || url !== displayUrl(source)) return
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
    stopVideo()
    stopPreloadWatch?.()
    stopPreloadWatch = undefined
    const active = () => change === generation && tempStore.lightbox.visible && tempStore.lightbox.sources === sources
    let metadata: ((event: Event) => void) | undefined
    if (isVideo(source.name)) {
      let element: HTMLVideoElement | undefined
      let release: (() => void) | undefined
      let resolve!: (video?: HTMLVideoElement) => void
      const ready = new Promise<HTMLVideoElement | undefined>(done => { resolve = done })
      metadata = event => {
        if (!active() || element) return
        element = event.currentTarget as HTMLVideoElement
        resolve(element)
      }
      retireVideo = () => {
        resolve()
        element?.pause()
        release?.()
      }
      const mediaId = source.type === DataType.VIDEO ? source.data?.id : undefined
      void history.restoreAndPlay(ready, mediaId ? { mediaId, type: 'VIDEO' } : undefined, active).then(stop => {
        if (active()) release = stop
        else stop?.()
      }).catch(error => {
        if (!active()) return
        status.loading = false
        status.loadError = true
        emitter.emit('toast', error instanceof Error ? error.message : HttpErrorCode.NetworkError)
      })
    }
    const oldIndex = imgIndex.value
    imgIndex.value = newIndex
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
      const alreadyDecoded = (isImage(source.name) && decodedUrls.has(displayUrl(source) ?? '')) ||
        (isVideo(source.name) && readyVideoUrls.has(source.src))
      reset(alreadyDecoded)
      current.value = source
      if (metadata) videoLoad.value = { key: change, loaded: metadata }
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
      stopVideo()
      release(source)
      trimPreloads()
      status.loading = false
      status.loadError = true
      emit('on-error', error)
    }
  }

  const imgSrc = ref<string>()
  watch([current, imageViewQuality], async ([source], previous, onCleanup) => {
    let active = true
    onCleanup(() => { active = false })
    if (source !== previous[0]) imgSrc.value = undefined
    if (!source || !isImage(source.name)) return
    if (source === previous[0]) {
      trimPreloads()
      for (const neighbour of neighbours(tempStore.lightbox.sources, imgIndex.value)) void preloadSource(neighbour)
    }
    try {
      const url = await resolveDisplayUrl(source)
      if (active) {
        status.loadError = false
        imgSrc.value = url
      }
    } catch (error) {
      if (active) {
        status.loading = false
        status.loadError = true
        emit('on-error', error)
      }
    }
  })

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

  return { imgSrc, videoLoad, reset, closeDialog, changeIndex, onNext, onPrev }
}

export function useLightboxFileActions(
  current: Ref<ISource | undefined>,
  refetchInfo: () => void,
  isPhone: boolean,
  lightboxInfoVisible: Ref<boolean>,
) {
  const { t } = useI18n()
  const deleteItem = useDeleteMediaItem()
  const { renameItem, renamedPath, renameDone, renameMutation, renameVariables } = useRename(() => {
    refetchInfo()
  })

  function downloadFile() {
    const source = current.value
    if (!source) return
    download(source.src, (source.name || getFileName(source.path)).replace(' ', '-'), source.isFromChat, source.path)
  }

  const { mutate: deleteFiles, onError: onDeleteError } = initMutation({ document: deleteFilesGQL }, showActionError)
  onDeleteError(refetchInfo)

  async function deleteFile(): Promise<boolean> {
    const source = current.value
    if (!source?.data) return false
    const { type, data: item, path } = source
    if (type === DataType.VIDEO || type === DataType.AUDIO || type === DataType.IMAGE) {
      return !!await deleteItem(type, item)
    }
    if (!await deleteFiles({ paths: [path] })) return false
    emitter.emit('file_deleted', { item })
    return true
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
  closeDialog: () => void,
  onNext: () => void,
  onPrev: () => void,
  changeIndex: (i: number) => void,
  emit: (event: string, ...args: any[]) => void,
  popup: Ref<boolean> = ref(false),
) {
  let isVideoPlaying = true

  const onLoad = () => {
    status.loading = false
    if (current.value && isImage(current.value.name)) {
      setImgSize()
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
    off(window, 'keydown', onKeyPress)
    off(window, 'resize', onWindowResize)
    emitter.off('item_tags_updated', itemTagsUpdatedHandler)
    emitter.off('media_items_actioned', mediaItemsActionedHandler)
    emitter.off('file_deleted', fileDeletedHandler)
    emitter.off('file_renamed', fileRenamedHandler)
  })

  return { onLoad, onError, onPlaying, onPause, onVolumeChange }
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
