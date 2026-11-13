import { expect, it, vi } from 'vitest'
import { computed, ref } from 'vue'

const fixture = vi.hoisted(() => ({ mounted: [] as Array<() => void>, unmounted: [] as Array<() => void>, callbacks: new Map<string, () => void>(), fetch: vi.fn(), uploads: vi.fn(), enqueue: vi.fn() }))
vi.mock('vue', async original => ({
  ...await original<typeof import('vue')>(),
  onMounted: (fn: () => void) => fixture.mounted.push(fn),
  onUnmounted: (fn: () => void) => fixture.unmounted.push(fn),
}))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (value: string) => value }) }))
vi.mock('@/components/toaster', () => ({ default: vi.fn() }))
vi.mock('@/hooks/upload', () => ({ useChatFilesUpload: () => ({ getUploads: fixture.uploads }) }))
vi.mock('@/views/chat/hooks/chat', () => ({ useTasks: () => ({ enqueue: fixture.enqueue, cancel: vi.fn() }) }))
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
    return { data: { downloads: [] } }
  }).mockResolvedValue({ data: { downloads: [
    { id: 'task', source: { __typename: 'ChatDownloadSource', messageId: 'message' }, downloadedBytes: 4, totalBytes: 10, bytesPerSecond: 1, status: 'DOWNLOADING' },
  ] } })
  const result = useChatUpload(computed(() => 'peer:one'), computed(() => ''), '/owned', () => {}, ref(''), ref([]))
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

it('keeps a Unicode attachment summary valid at the UTF-16 truncation boundary', async () => {
  const text = 'owned-'.padEnd(47, 'x') + '中文🦀\n'.repeat(750)
  const draft = ref(text)
  fixture.uploads.mockImplementation((_dir, files: File[]) => files.map((file, index) => ({ id: 'long-' + index, file, uploadedSize: 0, uploadSpeed: 0 })))
  fixture.enqueue.mockImplementation(async (item, uploads) => {
    const summary = item._content.value.items[0].summary as string
    expect(summary).toBe(new TextDecoder().decode(new TextEncoder().encode(summary)))
    expect(summary.length).toBeLessThanOrEqual(253)
    expect(text.startsWith(summary.slice(0, -3))).toBe(true)
    expect(await uploads[0].file.text()).toBe(text)
  })
  const upload = useChatUpload(computed(() => 'local'), computed(() => ''), '/owned', () => {}, draft, ref([]))
  await upload.sendLongMessageAsFile(text)
  expect(fixture.enqueue).toHaveBeenCalledTimes(1)
  expect(draft.value).toBe('')
})
