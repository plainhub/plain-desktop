import { ref } from 'vue'
import { getGraphqlContextKey, gqlFetch } from '@/lib/api/gql-client'
import { audioQueueGQL } from '@/lib/api/query'
import type { IAudioItem } from '@/lib/interfaces'

const PAGE_SIZE = 200

export const audioPlaylistItems = ref<IAudioItem[]>([])
export const audioPlaylistTotal = ref(0)
export const audioPlaylistLoading = ref(false)

export interface IAudioPlayback {
  mediaId: string | null
  path: string | null
}

function idleAudioPlayback(): IAudioPlayback {
  return { mediaId: null, path: null }
}

export const audioPlayback = ref<IAudioPlayback>(idleAudioPlayback())

export const audioPlayMode = ref<'REPEAT' | 'REPEAT_ONE' | 'SHUFFLE'>('REPEAT')

let fetchSeq = 0
let initialFetched = false

interface IAudioQueuePage {
  total: number
  items: IAudioItem[]
  playback?: IAudioPlayback
  prefs?: { audio_play_mode?: unknown }
}

async function fetchPage(offset: number, limit = PAGE_SIZE): Promise<boolean> {
  const context = getGraphqlContextKey()
  const playback = audioPlayback.value
  const mode = audioPlayMode.value
  const seq = ++fetchSeq
  audioPlaylistLoading.value = true
  try {
    const r = await gqlFetch<IAudioQueuePage>(audioQueueGQL, { offset, limit })
    if (seq !== fetchSeq || context !== getGraphqlContextKey()) return false
    if (r.errors?.length || !r.data?.items) return false
    const page = r.data
    if (page.playback && audioPlayback.value === playback) audioPlayback.value = page.playback
    const storedMode = page.prefs?.audio_play_mode
    if (audioPlayMode.value === mode && page.prefs) {
      audioPlayMode.value = storedMode === 'SHUFFLE' || storedMode === 'REPEAT_ONE' ? storedMode : 'REPEAT'
    }
    audioPlaylistTotal.value = page.total
    if (offset === 0) {
      audioPlaylistItems.value = page.items
    } else {
      const known = new Set(audioPlaylistItems.value.map((it) => it.mediaId))
      audioPlaylistItems.value = [...audioPlaylistItems.value, ...page.items.filter((it) => !known.has(it.mediaId))]
    }
    return true
  } catch {
    return false
  } finally {
    if (seq === fetchSeq) audioPlaylistLoading.value = false
  }
}

export function useAudioPlaylistStore() {
  async function fetchInitial() {
    initialFetched = await fetchPage(0)
  }

  async function ensureLoaded() {
    if (!initialFetched) await fetchInitial()
  }

  async function loadMore() {
    if (audioPlaylistItems.value.length < audioPlaylistTotal.value) {
      await fetchPage(audioPlaylistItems.value.length)
    }
  }

  async function refetch() {
    await fetchPage(0, Math.max(PAGE_SIZE, audioPlaylistItems.value.length))
  }

  function removeLocal(mediaId: string) {
    audioPlaylistItems.value = audioPlaylistItems.value.filter((it) => it.mediaId !== mediaId)
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
  audioPlayMode.value = 'REPEAT'
  fetchSeq = 0
  initialFetched = false
}
