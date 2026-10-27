import { expect, it, vi } from 'vitest'
import { computed, ref } from 'vue'

const fixture = vi.hoisted(() => ({ mounted: [] as Array<() => void>, unmounted: [] as Array<() => void>, callbacks: new Map<string, () => void>(), fetch: vi.fn() }))
vi.mock('vue', async original => ({
  ...await original<typeof import('vue')>(),
  onMounted: (fn: () => void) => fixture.mounted.push(fn),
  onUnmounted: (fn: () => void) => fixture.unmounted.push(fn),
}))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (value: string) => value }) }))
vi.mock('@/components/toaster', () => ({ default: vi.fn() }))
vi.mock('@/hooks/upload', () => ({ useChatFilesUpload: () => ({ getUploads: vi.fn() }) }))
vi.mock('@/views/chat/hooks/chat', () => ({ useTasks: () => ({ enqueue: vi.fn(), cancel: vi.fn() }) }))
vi.mock('@/lib/api/gql-client', () => ({ gqlFetch: fixture.fetch }))
vi.mock('@/lib/api/subscription-client', () => ({ graphqlSubscriptions: {
  subscribe: (_name: string, callback: () => void) => { fixture.callbacks.set('changed', callback); return () => fixture.callbacks.delete('changed') },
  onAttached: (callback: () => void) => { fixture.callbacks.set('attached', callback); return () => fixture.callbacks.delete('attached') },
} }))

import { useChatUpload } from '../../src/views/chat/hooks/chat-upload'

it('coalesces progress notifications and applies the latest queue snapshot', async () => {
  let finish!: () => void
  const blocked = new Promise<void>(resolve => { finish = resolve })
  fixture.fetch.mockImplementationOnce(async () => {
    await blocked
    return { data: { chatDownloadQueue: { tasks: [] } } }
  }).mockResolvedValue({ data: { chatDownloadQueue: { tasks: [
    { messageId: 'message', downloaded: 4, total: 10, speed: 1, status: 'DOWNLOADING' },
  ] } } })
  const result = useChatUpload(computed(() => 'peer:one'), computed(() => ''), ref(''), () => {}, ref(''), ref([]))
  fixture.mounted.forEach(fn => fn())
  const changed = fixture.callbacks.get('changed')!
  changed()
  changed()
  expect(fixture.fetch).toHaveBeenCalledTimes(1)
  finish()
  await vi.waitFor(() => expect(result.downloadProgress.message?.downloaded).toBe(4))
  expect(fixture.fetch).toHaveBeenCalledTimes(2)
  fixture.unmounted.forEach(fn => fn())
  expect(fixture.callbacks.size).toBe(0)
})
