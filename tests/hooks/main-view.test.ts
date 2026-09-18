import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { defineComponent } from 'vue'
import { flushPromises, mount } from '@vue/test-utils'

vi.mock('vue-router', async () => {
  const { ref } = await vi.importActual<typeof import('vue')>('vue')
  return {
    useRouter: () => ({ currentRoute: ref({ fullPath: '/', path: '/', matched: [] }) }),
  }
})

const gqlFetchMock = vi.fn()
vi.mock('@/lib/api/gql-client', () => {
  class GqlError extends Error {}
  return { gqlFetch: (...args: any[]) => gqlFetchMock(...args), GqlError }
})

vi.mock('@/lib/api/query', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/api/query')>()),
  audioQueueGQL: 'audioQueue-query',
}))

import { useMainView } from '@/hooks/main-view'
import { resetAudioPlaylistForTests } from '@/hooks/audio-playlist-store'
import emitter from '@/plugins/eventbus'

beforeEach(() => {
  setActivePinia(createPinia())
  resetAudioPlaylistForTests()
  gqlFetchMock.mockReset()
  gqlFetchMock.mockImplementation(async (doc: string) => {
    if (doc.includes('audioQueue-query')) return { data: { items: [], total: 0 } }
    return { data: { app: { clientId: 'c1', deviceName: '', urlToken: '' } } }
  })
})

function mountMainView() {
  return mount(defineComponent({
    setup() {
      useMainView()
      return () => null
    },
  }))
}

describe('useMainView media_items_actioned', () => {
  it('refetches the audio playlist mirror when audios are actioned', async () => {
    mountMainView()
    await flushPromises()
    const callsBefore = gqlFetchMock.mock.calls.length

    emitter.emit('media_items_actioned', { type: 'AUDIO', action: 'delete', query: 'ids:1' })
    await flushPromises()

    expect(gqlFetchMock.mock.calls.length).toBe(callsBefore + 1)
    expect(gqlFetchMock.mock.calls.at(-1)![0]).toBe('audioQueue-query')
  })

  it('does not touch the audio playlist for other media types', async () => {
    mountMainView()
    await flushPromises()
    const callsBefore = gqlFetchMock.mock.calls.length

    emitter.emit('media_items_actioned', { type: 'IMAGE', action: 'delete', query: 'ids:1' })
    await flushPromises()

    expect(gqlFetchMock.mock.calls.length).toBe(callsBefore)
  })
})
