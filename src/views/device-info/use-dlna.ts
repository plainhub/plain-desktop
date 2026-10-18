import { ref } from 'vue'
import { gqlFetch } from '@/lib/api/gql-client'
import { isLocalMode } from '@/lib/device/local-mode'

export type DlnaMediaType = 'VIDEO' | 'AUDIO' | 'IMAGE' | 'UNKNOWN'

export interface DlnaStateSnapshot {
  enabled: boolean
  isRunning: boolean
  isRetrying: boolean
  port: number
  mediaUri: string
  mediaTitle: string
  mediaAlbumArtUri: string
  mediaType: DlnaMediaType
  playbackState: string
  currentPositionMs: number
  durationMs: number
  pendingCastRequest: PendingCastRequest | null
  startError: string
}

export interface PendingCastRequest {
  senderIp: string
  senderName: string
  mediaUri: string
  mediaTitle: string
  mediaType: DlnaMediaType
  albumArtUri: string
}

export function useDlna() {
  const snapshot = ref<DlnaStateSnapshot | null>(null)
  const loading = ref(false)

  async function load() {
    if (!isLocalMode()) return
    try {
      const result = await gqlFetch<{ dlnaReceiver: DlnaStateSnapshot }>('query { dlnaReceiver }')
      if (result.errors?.length) throw new Error(result.errors[0].message)
      snapshot.value = result.data?.dlnaReceiver ?? null
    } catch (e) {
      console.error('DLNA receiver query failed', e)
    }
  }

  async function setEnabled(enabled: boolean) {
    if (!isLocalMode()) return
    loading.value = true
    try {
      const result = await gqlFetch('mutation($key: String!, $value: JSON!) { setUserPref(key: $key, value: $value) }', { key: 'dlna', value: enabled })
      if (result.errors?.length) throw new Error(result.errors[0].message)
      await load()
    } catch (e) {
      console.error('DLNA receiver control failed', e)
    } finally {
      loading.value = false
    }
  }

  return { snapshot, loading, load, setEnabled }
}
