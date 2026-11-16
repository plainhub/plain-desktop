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
  const result = useChatUpload(computed(() => 'peer:one'), computed(() => ''), '/owned', () => {}, ref([]))
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

it('keeps a long Unicode caption and ordered mixed attachments together during upload', async () => {
  const text = '原始文字🦀\n'.repeat(750)
  const files = [new File(['document'], 'one.pdf'), new File(['image'], 'two.bin'), new File(['notes'], 'three.txt')]
  const attachments = files.map((file, index) => ({ type: index === 1 ? 'IMAGE' : 'FILE', file } as const))
  fixture.uploads.mockImplementation((_dir, source: File[]) => source.map((file, index) => ({ id: 'mixed-' + index, file, uploadedSize: 0, uploadSpeed: 0 })))
  fixture.enqueue.mockImplementation(async (item, uploads) => {
    expect(item.content).toMatchObject({ __typename: 'ChatMessageContent', text, format: 'MARKDOWN' })
    expect(item.content.items.map((i: any) => [i.type, i.attachment.fileName])).toEqual([['FILE', 'one.pdf'], ['IMAGE', 'two.bin'], ['FILE', 'three.txt']])
    expect(uploads.map((upload: any) => upload.file)).toEqual(files)
  })
  const upload = useChatUpload(computed(() => 'local'), computed(() => ''), '/owned', () => {}, ref([]))
  await upload.uploadMessage(attachments, text, 'MARKDOWN')
  expect(fixture.enqueue).toHaveBeenCalledTimes(1)
})
