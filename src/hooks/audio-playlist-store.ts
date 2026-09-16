import { ref } from 'vue'
import { gqlFetch } from '@/lib/api/gql-client'
import { audioPlaylistGQL } from '@/lib/api/query'
import type { IPlaylistAudio } from '@/lib/interfaces'

const PAGE_SIZE = 200

/**
 * Reactive view of the phone playback queue (`audioPlaylist` query).
 *
 * The queue is server-side state (a playback source plus manual items), so
 * this store only mirrors a window of it: mutate via GraphQL mutations, then
 * refetch. Module-level singletons — every consumer shares one list.
 */
export const audioPlaylistItems = ref<IPlaylistAudio[]>([])
export const audioPlaylistTotal = ref(0)
export const audioPlaylistLoading = ref(false)

let fetchSeq = 0
let initialFetched = false

interface IAudioPlaylistPage {
  total: number
  items: IPlaylistAudio[]
}

async function fetchPage(offset: number) {
  const seq = ++fetchSeq
  audioPlaylistLoading.value = true
  try {
    const r = await gqlFetch<{ audioPlaylist: IAudioPlaylistPage }>(audioPlaylistGQL, { offset, limit: PAGE_SIZE })
    if (seq !== fetchSeq) return // superseded by a newer fetch
    if (r.errors?.length || !r.data?.audioPlaylist) return
    const page = r.data.audioPlaylist
    audioPlaylistTotal.value = page.total
    if (offset === 0) {
      audioPlaylistItems.value = page.items
    } else {
      // Merge only paths not already mirrored locally (mutations may have
      // changed the tail while older pages are still loading).
      const known = new Set(audioPlaylistItems.value.map((it) => it.path))
      audioPlaylistItems.value = [...audioPlaylistItems.value, ...page.items.filter((it) => !known.has(it.path))]
    }
  } finally {
    if (seq === fetchSeq) audioPlaylistLoading.value = false
  }
}

export function useAudioPlaylistStore() {
  async function fetchInitial() {
    initialFetched = true
    await fetchPage(0)
  }

  /** Load once per session; safe to call on every mount. */
  async function ensureLoaded() {
    if (!initialFetched) await fetchInitial()
  }

  async function loadMore() {
    if (audioPlaylistItems.value.length < audioPlaylistTotal.value) {
      await fetchPage(audioPlaylistItems.value.length)
    }
  }

  async function refetch() {
    await fetchPage(0)
  }

  /** Local mirror after a mutation; call refetch() to get the server truth. */
  function removeLocal(path: string) {
    audioPlaylistItems.value = audioPlaylistItems.value.filter((it) => it.path !== path)
    audioPlaylistTotal.value = Math.max(0, audioPlaylistTotal.value - 1)
  }

  function reset() {
    audioPlaylistItems.value = []
    audioPlaylistTotal.value = 0
    initialFetched = false
  }

  return {
    items: audioPlaylistItems,
    total: audioPlaylistTotal,
    loading: audioPlaylistLoading,
    fetchInitial,
    ensureLoaded,
    loadMore,
    refetch,
    removeLocal,
    reset,
  }
}
