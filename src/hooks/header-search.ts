import { computed, onMounted, onUnmounted, reactive, ref, watch, type Ref } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { useMainStore } from '@/stores/main'
import { replacePath } from '@/plugins/router'
import { parseQuery } from '@/lib/search'
import { decodeBase64, encodeBase64 } from '@/lib/strutil'
import { isEditableTarget } from '@/lib/dom'
import type { IBucket, IFeed, IFilter, IFileFilter, ITag } from '@/lib/interfaces'
import { DataType } from '@/lib/data'
import { useBucketsTags } from '@/hooks/media'
import { useTags } from '@/hooks/tags'
import { feedsGQL, initLazyQuery } from '@/lib/api/query'
import { useSearchHistory } from '@/components/header-search/history'
import { formatHistoryLabel } from '@/components/header-search/formatHistoryLabel'
import { keyOptionsForGroup, valueOptionsForGroup } from '@/components/header-search/options'
import { decodedQuery, parseCurrentFields } from '@/components/header-search/q'
import { useHeaderSearchQuery } from '@/hooks/header-search-query'
import { useHeaderSearchTokens } from '@/hooks/header-search-filters'

type Kind = 'global' | 'media' | 'files'

export function useHeaderSearch(props: {
  kind: Kind
  placeholder: string
  enableSlashFocus: boolean
  targetPath: string
  syncRouteQ: boolean
  filter?: IFilter
  getUrl?: (q: string) => string
  tags: ITag[]
  buckets: IBucket[]
  showTrash: boolean
  fileFilter?: IFileFilter
  getFileUrl?: (q: string) => string
  navigateToDir: (dir: string) => void
}) {
  const router = useRouter()
  const mainStore = useMainStore()
  const { t } = useI18n()

  const inputRef = ref<{ focus: () => void } | null>(null)
  const text = ref('')

  const {
    buildMediaQ, copyMediaFilter, parseMediaQ,
    buildFilesQ, parseFilesQ,
    buildNextQ, buildNextMediaQ, buildNextMessagesQ, buildNextCallsQ, buildNextFeedsQ, buildNextDocsQ,
  } = useHeaderSearchQuery()

  const mediaLocalFilter: IFilter = reactive({ tagIds: [] })
  const filesLocalFilter: IFileFilter = reactive({ showHidden: false, type: '', rootPath: '', parent: '', text: '', fileSize: undefined })
  const callsLocal = reactive({ duration: '', startTime: '' })
  const docsExt = ref('')

  const routeGroup = computed(() => String(router.currentRoute.value.meta?.group ?? ''))
  const showMediaFilters = computed(() => ['audios', 'videos', 'images'].includes(routeGroup.value))
  const showFilesFilters = computed(() => routeGroup.value === 'files')
  const showDocsFilters = computed(() => routeGroup.value === 'docs')
  const showAppsFilters = computed(() => routeGroup.value === 'apps')
  const showMessagesFilters = computed(() => routeGroup.value === 'messages')
  const showCallsFilters = computed(() => routeGroup.value === 'calls')
  const showFeedsFilters = computed(() => routeGroup.value === 'feeds')

  const resolvedPlaceholder = computed(() => props.placeholder || t('search_hint'))

  // Data fetching
  const { tags: audioTags, buckets: audioBuckets, fetch: fetchAudioBucketsTags } = useBucketsTags(DataType.AUDIO)
  const { tags: videoTags, buckets: videoBuckets, fetch: fetchVideoBucketsTags } = useBucketsTags(DataType.VIDEO)
  const { tags: imageTags, buckets: imageBuckets, fetch: fetchImageBucketsTags } = useBucketsTags(DataType.IMAGE)
  const mediaTags = computed<ITag[]>(() => {
    if (routeGroup.value === 'audios') return audioTags.value
    if (routeGroup.value === 'videos') return videoTags.value
    if (routeGroup.value === 'images') return imageTags.value
    return []
  })
  const mediaBuckets = computed<IBucket[]>(() => {
    if (routeGroup.value === 'audios') return audioBuckets.value
    if (routeGroup.value === 'videos') return videoBuckets.value
    if (routeGroup.value === 'images') return imageBuckets.value
    return []
  })
  const { tags: messageTags, fetch: fetchMessageTags } = useTags(DataType.SMS)
  const feeds = ref<IFeed[]>([])
  const { fetch: fetchFeeds } = initLazyQuery({
    handle: (data: { feeds: IFeed[] }, error: string) => { if (!error) feeds.value = (data?.feeds ?? []) as IFeed[] },
    document: feedsGQL,
    variables: () => ({}),
  })

  const kind = computed<Kind>(() => props.kind ?? 'global')
  const currentEncodedQ = computed(() => router.currentRoute.value.query.q?.toString() ?? '')

  const { historyQ, historyValueOptions, rememberHistoryDecoded, rememberHistoryBase64, deleteHistoryItem, clearHistoryForPage } =
    useSearchHistory({
      max: 10, decodeBase64,
      formatLabel: (decoded) => formatHistoryLabel({ decoded, group: routeGroup.value, t, mediaBuckets: mediaBuckets.value ?? [], mediaTags: mediaTags.value ?? [], feeds: feeds.value ?? [], messageTags: messageTags.value ?? [] }),
    })

  const keyOptions = computed(() => keyOptionsForGroup(routeGroup.value, (historyQ.value ?? []).length > 0))
  const valueOptions = computed<Record<string, any[]>>(() =>
    valueOptionsForGroup({ group: routeGroup.value, t, history: historyValueOptions.value, mediaTags: mediaTags.value ?? [], mediaBuckets: mediaBuckets.value ?? [], feeds: feeds.value ?? [], messageTags: messageTags.value ?? [] }),
  )

  const { uiTokens, onUiTokensChange } = useHeaderSearchTokens(
    routeGroup, text, mediaLocalFilter, filesLocalFilter, callsLocal,
    mediaTags, mediaBuckets, messageTags, feeds,
    showMediaFilters, showFilesFilters, showAppsFilters, showMessagesFilters, showCallsFilters, showFeedsFilters, showDocsFilters,
    docsExt,
  )

  // Route sync
  function replaceCurrentRouteQ(q: string) {
    const route = router.currentRoute.value
    const targetPath = props.targetPath || (route.path === '/' ? '/files' : route.path)
    const nextQuery: Record<string, any> = { ...route.query }
    delete nextQuery.page; delete nextQuery.q
    if (q) nextQuery.q = q
    replacePath(mainStore, router.resolve({ path: targetPath, query: nextQuery }).fullPath)
    if (q) rememberHistoryBase64(q)
  }

  function syncFromRoute() {
    if (!props.syncRouteQ) return
    const fields = parseCurrentFields(currentEncodedQ.value)
    text.value = fields.find((it) => it.name === 'text')?.value ?? ''
    const df = fields.find((it) => it.name === 'duration')
    callsLocal.duration = df ? `${df.op}${df.value}` : ''
    const sf = fields.find((it) => it.name === 'start_time')
    callsLocal.startTime = sf ? `${sf.op}${sf.value}` : ''
    docsExt.value = fields.find((it) => it.name === 'ext')?.value ?? ''
    const decoded = decodedQuery(currentEncodedQ.value)
    parseMediaQ(mediaLocalFilter, decoded)
    parseFilesQ(filesLocalFilter, decoded)
  }

  function applyHistoryQ(q: string) {
    if (!q) return
    try { const fields = parseQuery(q); text.value = fields.find((it) => it.name === 'text')?.value ?? '' } catch { /* ignore */ }
    parseMediaQ(mediaLocalFilter, q); parseFilesQ(filesLocalFilter, q)
    const qBase64 = encodeBase64(q)
    if (kind.value === 'global') { replaceCurrentRouteQ(qBase64); return }
    if (kind.value === 'media' && props.getUrl) { replacePath(mainStore, props.getUrl(qBase64)); rememberHistoryDecoded(q); return }
    if (kind.value === 'files' && props.getFileUrl) { replacePath(mainStore, props.getFileUrl(qBase64)); rememberHistoryDecoded(q) }
  }

  // Submit
  function submitGlobal(value?: string) {
    mediaLocalFilter.text = value ?? text.value; filesLocalFilter.text = value ?? text.value
    replaceCurrentRouteQ(buildNextQ(currentEncodedQ.value, value ?? text.value, filesLocalFilter.showHidden))
  }

  function submitFromHeader() {
    if (kind.value === 'global') {
      if (showMediaFilters.value) { replaceCurrentRouteQ(buildNextMediaQ(currentEncodedQ.value, { ...mediaLocalFilter, text: text.value })); return }
      if (showAppsFilters.value) { replaceCurrentRouteQ(buildNextMediaQ(currentEncodedQ.value, { tagIds: [], type: mediaLocalFilter.type, text: text.value })); return }
      if (showFilesFilters.value) { replaceCurrentRouteQ(buildFilesQ({ ...filesLocalFilter, text: text.value })); return }
      if (showDocsFilters.value) { replaceCurrentRouteQ(buildNextDocsQ(currentEncodedQ.value, filesLocalFilter.fileSize, docsExt.value, text.value)); return }
      if (showMessagesFilters.value) { replaceCurrentRouteQ(buildNextMessagesQ(currentEncodedQ.value, mediaLocalFilter.type, mediaLocalFilter.tagIds ?? [], text.value)); return }
      if (showCallsFilters.value) { replaceCurrentRouteQ(buildNextCallsQ(currentEncodedQ.value, mediaLocalFilter.type, callsLocal.duration, callsLocal.startTime, text.value)); return }
      if (showFeedsFilters.value) { replaceCurrentRouteQ(buildNextFeedsQ(currentEncodedQ.value, mediaLocalFilter.feedId, text.value)); return }
      submitGlobal(text.value); return
    }
    // Panel mode (media / files)
    if (kind.value === 'media' && props.filter && props.getUrl) {
      const q = buildMediaQ({ ...props.filter, text: text.value, tagIds: [...(mediaLocalFilter.tagIds ?? [])], bucketId: mediaLocalFilter.bucketId, type: mediaLocalFilter.type, trash: mediaLocalFilter.trash })
      replacePath(mainStore, props.getUrl(q)); if (q) rememberHistoryBase64(q); return
    }
    if (kind.value === 'files' && props.fileFilter && props.getFileUrl) {
      const inputText = text.value.trim()
      if (inputText && inputText.startsWith('/') && props.navigateToDir) { props.navigateToDir(inputText); return }
      const q = buildFilesQ({ ...props.fileFilter, ...filesLocalFilter, text: text.value })
      replacePath(mainStore, props.getFileUrl(q)); if (q) rememberHistoryBase64(q)
    }
  }

  function onFreeTextChange(v: string) { text.value = v }

  // Keyboard
  function onGlobalKeydown(event: KeyboardEvent) {
    if (!props.enableSlashFocus || event.key !== '/' || event.ctrlKey || event.metaKey || event.altKey) return
    if (isEditableTarget(event.target)) return
    event.preventDefault(); inputRef.value?.focus()
  }

  // Watchers
  watch(() => router.currentRoute.value.fullPath, () => syncFromRoute(), { immediate: true })
  watch(routeGroup, (g) => {
    if (g === 'audios') fetchAudioBucketsTags()
    else if (g === 'videos') fetchVideoBucketsTags()
    else if (g === 'images') fetchImageBucketsTags()
    else if (g === 'feeds') fetchFeeds()
    else if (g === 'messages') fetchMessageTags()
  }, { immediate: true })
  watch(() => props.filter, (v) => { if (!v) return; copyMediaFilter(v, mediaLocalFilter); text.value = v.text ?? '' }, { immediate: true, deep: true })
  watch(() => props.fileFilter, (v) => { if (!v) return; Object.assign(filesLocalFilter, v); text.value = v.text ?? '' }, { immediate: true, deep: true })

  onMounted(() => window.addEventListener('keydown', onGlobalKeydown))
  onUnmounted(() => window.removeEventListener('keydown', onGlobalKeydown))

  return {
    inputRef, text, uiTokens, resolvedPlaceholder, keyOptions, valueOptions,
    onFreeTextChange, onUiTokensChange, submitFromHeader, applyHistoryQ, deleteHistoryItem, clearHistoryForPage,
  }
}
