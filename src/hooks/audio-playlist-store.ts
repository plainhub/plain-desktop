import { ref } from 'vue'
import { gqlFetch } from '@/lib/api/gql-client'
import { audioQueueGQL } from '@/lib/api/query'
import type { IAudioItem } from '@/lib/interfaces'

const PAGE_SIZE = 200

/**
 * Reactive view of the phone playback queue (`audioQueue` query).
 *
 * The queue is server-side state (a playback source plus manual items), so
 * this store only mirrors a window of it: mutate via GraphQL mutations, then
 * refetch. Module-level singletons — every consumer shares one list.
 */
export const audioPlaylistItems = ref<IAudioItem[]>([])
export const audioPlaylistTotal = ref(0)
export const audioPlaylistLoading = ref(false)

export interface IAudioPlayback {
  mode: string
  currentPath: string | null
  isPlaying: boolean
  positionMs: number
}

function idleAudioPlayback(): IAudioPlayback {
  return { mode: 'REPEAT', currentPath: null, isPlaying: false, positionMs: 0 }
}

/**
 * Player state mirrored from the `audioPlayback` root field of the queue
 * query: play mode + current track path. Mutations update it optimistically,
 * the next refetch re-syncs with the server.
 */
export const audioPlayback = ref<IAudioPlayback>(idleAudioPlayback())

let fetchSeq = 0
let initialFetched = false

interface IAudioQueuePage {
  total: number
  items: IAudioItem[]
  playback?: IAudioPlayback
}

async function fetchPage(offset: number): Promise<boolean> {
  const seq = ++fetchSeq
  audioPlaylistLoading.value = true
  try {
    const r = await gqlFetch<IAudioQueuePage>(audioQueueGQL, { offset, limit: PAGE_SIZE })
    if (seq !== fetchSeq) return false // superseded by a newer fetch
    if (r.errors?.length || !r.data?.items) return false
    const page = r.data
    if (page.playback) audioPlayback.value = page.playback
    audioPlaylistTotal.value = page.total
    if (offset === 0) {
      audioPlaylistItems.value = page.items
    } else {
      // Merge only paths not already mirrored locally (mutations may have
      // changed the tail while older pages are still loading).
      const known = new Set(audioPlaylistItems.value.map((it) => it.path))
      audioPlaylistItems.value = [...audioPlaylistItems.value, ...page.items.filter((it) => !known.has(it.path))]
    }
    return true
  } catch {
    // Network failures leave the mirror as-is; a later refetch re-syncs.
    return false
  } finally {
    if (seq === fetchSeq) audioPlaylistLoading.value = false
  }
}

export function useAudioPlaylistStore() {
  async function fetchInitial() {
    initialFetched = await fetchPage(0)
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
    audioPlayback.value = idleAudioPlayback()
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

export function resetAudioPlaylistForTests() {
  audioPlaylistItems.value = []
  audioPlaylistTotal.value = 0
  audioPlaylistLoading.value = false
  audioPlayback.value = idleAudioPlayback()
  fetchSeq = 0
  initialFetched = false
}
