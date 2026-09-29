import router, { replacePath } from '@/plugins/router'
import { useMainStore } from '@/stores/main'
import { computed, onMounted, onUnmounted, reactive, ref, watch } from 'vue'
import { useSearch } from '@/hooks/search'
import { decodeBase64, encodeBase64 } from '@/lib/strutil'
import type { IDocExtGroup, IFilter, IMediaItemsActionedEvent } from '@/lib/interfaces'
import { storeToRefs } from 'pinia'
import { useTempStore } from '@/stores/temp'
import { DataType } from '@/lib/data'
import { initLazyQuery } from '@/lib/api/query'
import { buildQuery, parseQuery } from '@/lib/search'
import emitter from '@/plugins/eventbus'

export function useMediaSidebar(type: DataType, gql: string) {
  const mainStore = useMainStore()
  const { counter, app } = storeToRefs(useTempStore())
  const { parseQ } = useSearch()
  const filter = reactive<IFilter>({ tagIds: [] })
  const group = ref('')
  const trash = ref(false)
  const selectedTagId = ref('')
  const selectedBucketId = ref('')
  const selectedExt = ref('')

  const total = computed(() => {
    if (type === DataType.IMAGE) return counter.value?.images ?? -1
    if (type === DataType.VIDEO) return counter.value?.videos ?? -1
    if (type === DataType.AUDIO) return counter.value?.audios ?? -1
    if (type === DataType.DOC) return counter.value?.docs ?? -1
    return -1
  })

  const totalTrash = computed(() => {
    if (type === DataType.IMAGE) return counter.value?.imagesTrash ?? -1
    if (type === DataType.VIDEO) return counter.value?.videosTrash ?? -1
    if (type === DataType.AUDIO) return counter.value?.audiosTrash ?? -1
    if (type === DataType.DOC) return counter.value?.docsTrash ?? -1
    return -1
  })

  const extGroups = computed<IDocExtGroup[]>(() => {
    if (type !== DataType.DOC) return []
    return counter.value?.docExtGroups ?? []
  })

  const { fetch } = initLazyQuery({
    handle: (data: { total: number; trash: number; extGroups?: IDocExtGroup[] }) => {
      if (!data) return
      if (type === DataType.IMAGE) { counter.value.images = data.total; counter.value.imagesTrash = data.trash }
      else if (type === DataType.VIDEO) { counter.value.videos = data.total; counter.value.videosTrash = data.trash }
      else if (type === DataType.AUDIO) { counter.value.audios = data.total; counter.value.audiosTrash = data.trash }
      else if (type === DataType.DOC) { counter.value.docs = data.total; counter.value.docsTrash = data.trash; counter.value.docExtGroups = data.extGroups ?? [] }
    },
    document: gql,
    variables: () => {
      const dirs = mainStore.excludedDirs
      const dirParts = dirs.map((d) => (d.includes(' ') ? `excluded_dir:"${d}"` : `excluded_dir:${d}`))
      return { query: dirParts.join(' ') }
    },
  })

  function updateActive() {
    const route = router.currentRoute.value
    group.value = route.meta.group || ''
    fetch()
    const q = decodeBase64(route.query.q?.toString() ?? '')
    parseQ(filter, q)
    selectedTagId.value = filter.tagIds.length === 1 ? filter.tagIds[0] : ''
    trash.value = filter.trash ?? false
    selectedBucketId.value = filter.bucketId ?? ''
    if (type === DataType.DOC) {
      const fields = parseQuery(q)
      const extField = fields.find((f) => f.name === 'ext')
      selectedExt.value = extField?.value ?? ''
    }
    if (selectedTagId.value && selectedBucketId.value) selectedTagId.value = ''
    if (trash.value) { selectedBucketId.value = ''; selectedTagId.value = ''; selectedExt.value = '' }
  }

  updateActive()
  watch(() => router.currentRoute.value.fullPath, () => updateActive())
  watch(() => mainStore.excludedDirs, () => fetch(), { deep: true })

  function viewTrash() {
    replacePath(mainStore, `/${group.value}?q=${encodeBase64(buildQuery([{ name: 'trash', op: '', value: 'true' }]))}`)
  }

  function viewAll() { replacePath(mainStore, `/${group.value}`) }

  function viewByExt(ext: string) {
    replacePath(mainStore, `/${group.value}?q=${encodeBase64(buildQuery([{ name: 'ext', op: '', value: ext }]))}`)
  }

  const mediaItemsActionedHandler = (event: IMediaItemsActionedEvent) => { if (event.type === type) fetch() }
  onMounted(() => emitter.on('media_items_actioned', mediaItemsActionedHandler))
  onUnmounted(() => emitter.off('media_items_actioned', mediaItemsActionedHandler))

  return { app, total, totalTrash, trash, selectedTagId, selectedBucketId, selectedExt, extGroups, viewAll, viewTrash, viewByExt }
}
