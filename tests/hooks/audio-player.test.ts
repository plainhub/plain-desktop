import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { defineComponent, nextTick, ref } from 'vue'
import { flushPromises, mount } from '@vue/test-utils'

const gqlFetchMock = vi.fn()
vi.mock('@/lib/api/gql-client', () => {
  class GqlError extends Error {}
  return { gqlFetch: (...args: any[]) => gqlFetchMock(...args), GqlError }
})

vi.mock('@/lib/api/query', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/api/query')>()),
  audioQueueGQL: 'audioQueue-query',
}))

import { useAudioPlaylist, usePlayAudio } from '@/hooks/audio-player'
import { audioPlayback, audioPlaylistItems, resetAudioPlaylistForTests } from '@/hooks/audio-playlist-store'
import { useTempStore } from '@/stores/temp'
import emitter from '@/plugins/eventbus'
import type { IAudioItem } from '@/lib/interfaces'

function audio(path: string): IAudioItem {
  return { title: path, artist: '', path, durationMs: 0 }
}

function mockGql(playlist: IAudioItem[]) {
  gqlFetchMock.mockImplementation(async (doc: string) => {
    if (doc.includes('mutation playAudio')) return { data: { playAudio: audio('/played.mp3') } }
    if (doc.includes('mutation clearAudioQueue')) return { data: { clearAudioQueue: true } }
    return { data: { items: playlist, total: playlist.length } }
  })
}

beforeEach(() => {
  setActivePinia(createPinia())
  resetAudioPlaylistForTests()
  gqlFetchMock.mockReset()
  const temp = useTempStore()
  temp.app = { clientId: '' } as any
  temp.urlTokenKey = new Uint8Array(32)
})

describe('usePlayAudio', () => {
  it('syncs audioCurrent from the mutation result and refetches the queue mirror', async () => {
    mockGql([audio('/played.mp3')])
    const applied = vi.fn()

    const { play } = usePlayAudio(applied)
    await play({ path: '/played.mp3' })
    await flushPromises()

    expect(audioPlayback.value.currentPath).toBe('/played.mp3')
    expect(gqlFetchMock).toHaveBeenCalledTimes(2)
    expect(gqlFetchMock.mock.calls[1][0]).toBe('audioQueue-query')
    expect(audioPlaylistItems.value.map((it) => it.path)).toEqual(['/played.mp3'])
    expect(applied).toHaveBeenCalledTimes(1)
  })

  it('keeps audioCurrent unchanged when the mutation fails', async () => {
    gqlFetchMock.mockImplementation(async (doc: string) => {
      if (doc.includes('mutation playAudio')) return { errors: [{ message: 'no permission' }] }
      return { data: { items: [], total: 0 } }
    })
    const applied = vi.fn()

    const { play } = usePlayAudio(applied)
    await play({ path: '/played.mp3' })
    await flushPromises()

    expect(audioPlayback.value.currentPath).toBeNull()
    expect(applied).not.toHaveBeenCalled()
  })
})

describe('useAudioPlaylist', () => {
  function mountPanel() {
    const audioEl = {
      play: vi.fn(),
      pause: vi.fn(),
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    }
    let hook: ReturnType<typeof useAudioPlaylist>
    const wrapper = mount(defineComponent({
      setup() {
        const audioRef = ref(audioEl as any)
        hook = useAudioPlaylist(audioRef)
        return () => null
      },
    }))
    return { hook: hook!, wrapper, audioEl }
  }

  it('plays the audio element after the queue mirror is synced', async () => {
    let queue = [audio('/a.mp3'), audio('/b.mp3')]
    gqlFetchMock.mockImplementation(async (doc: string) => {
      if (doc.includes('mutation playAudio')) {
        queue = [audio('/played.mp3')]
        return { data: { playAudio: audio('/played.mp3') } }
      }
      return { data: { items: queue, total: queue.length } }
    })
    const { hook, audioEl } = mountPanel()
    await flushPromises()
    expect(audioPlaylistItems.value).toHaveLength(2)

    hook.playItem(audio('/a.mp3'))
    await flushPromises()

    expect(audioPlayback.value.currentPath).toBe('/played.mp3')
    expect(audioPlaylistItems.value.map((it) => it.path)).toEqual(['/played.mp3'])
    expect(audioEl.play).toHaveBeenCalledTimes(1)
  })

  it('plays the audio element on the do_play_audio event (browser play flow)', async () => {
    mockGql([])
    const { audioEl } = mountPanel()
    await flushPromises()
    audioEl.play.mockClear()

    emitter.emit('do_play_audio')
    await nextTick()

    expect(audioEl.play).toHaveBeenCalledTimes(1)
  })

  it('resets the mirror only after clearAudioQueue succeeds', async () => {
    mockGql([audio('/a.mp3')])
    const { hook } = mountPanel()
    await flushPromises()
    expect(audioPlaylistItems.value).toHaveLength(1)

    gqlFetchMock.mockImplementation(async (doc: string) => {
      if (doc.includes('mutation clearAudioQueue')) return { errors: [{ message: 'failed' }] }
      return { data: { items: [audio('/a.mp3')], total: 1 } }
    })
    hook.clearPlaylist()
    await flushPromises()
    expect(audioPlaylistItems.value).toHaveLength(1)

    mockGql([])
    hook.clearPlaylist()
    await flushPromises()
    expect(audioPlaylistItems.value).toEqual([])
    expect(audioPlayback.value.currentPath).toBeNull()
  })
})
