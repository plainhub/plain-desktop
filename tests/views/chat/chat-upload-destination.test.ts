import { computed, ref } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { IUploadItem } from '@/stores/temp'

const mocks = vi.hoisted(() => ({
  enqueue: vi.fn(),
  cancel: vi.fn(),
  getUploads: vi.fn(),
  scrollBottom: vi.fn(),
}))

vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('@/hooks/upload', () => ({ useChatFilesUpload: () => ({ getUploads: mocks.getUploads }) }))
vi.mock('@/views/chat/hooks/chat', () => ({ useTasks: () => ({ enqueue: mocks.enqueue, cancel: mocks.cancel }) }))
vi.mock('@/lib/file', () => ({
  getImageData: vi.fn(async () => ({ width: 320, height: 200 })),
  getVideoData: vi.fn(),
  isVideo: () => false,
}))
vi.mock('@/plugins/eventbus', () => ({ default: { on: vi.fn(), off: vi.fn() } }))

import { snapshotChatUploadDestination, useChatUpload } from '@/views/chat/hooks/chat-upload'

describe('captured image upload destination', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mocks.enqueue.mockImplementation(async (item: any, _uploads: IUploadItem[], _toId: string, onSent?: (item: any) => void) => {
      onSent?.({ ...item, id: `sent-${item.id}` })
    })
    mocks.getUploads.mockImplementation((dir: string, files: File[]): IUploadItem[] =>
      files.map((file, index) => ({
        id: `upload-${index}`,
        dir,
        baseDir: dir,
        fileName: file.name,
        file,
        uploadedSize: 0,
        status: 'created',
        error: '',
      }))
    )
  })

  afterEach(() => {
    vi.restoreAllMocks()
  })

  it('uses one immutable destination for storage, optimistic metadata, and enqueue after route changes', async () => {
    const liveChatId = ref('peer:first')
    const liveChannelId = ref('')
    const chatItems = ref<any[]>([])
    const upload = useChatUpload(
      computed(() => liveChatId.value),
      computed(() => liveChannelId.value),
      '/live/app',
      mocks.scrollBottom,
      ref(''),
      chatItems
    )
    const destination = snapshotChatUploadDestination({
      chatId: liveChatId.value,
      channelId: liveChannelId.value,
      appDir: '/captured/app',
    })

    liveChatId.value = 'channel:later'
    liveChannelId.value = 'later'
    await upload.doUploadImages([new File(['png'], 'capture.png', { type: 'image/png' })], destination)

    expect(Object.isFrozen(destination)).toBe(true)
    expect(mocks.getUploads).toHaveBeenCalledWith('/captured/app', [expect.any(File)])
    expect(chatItems.value[0]).toMatchObject({ toId: 'peer:first', channelId: '' })
    expect(chatItems.value[0]._content.value.items[0]).toMatchObject({ dir: '/captured/app' })
    expect(mocks.enqueue).toHaveBeenCalledWith(expect.objectContaining({ id: expect.stringMatching(/^new_/) }), expect.any(Array), 'peer:first', expect.any(Function))
  })

  it('keeps the existing image caller by snapshotting current values at invocation', async () => {
    const chatId = ref('peer:current')
    const channelId = ref('')
    const upload = useChatUpload(
      computed(() => chatId.value),
      computed(() => channelId.value),
      '/live/app',
      mocks.scrollBottom,
      ref(''),
      ref<any[]>([])
    )

    await upload.doUploadImages([new File(['png'], 'capture.png', { type: 'image/png' })])

    expect(mocks.getUploads).toHaveBeenCalledWith('/live/app', [expect.any(File)])
    expect(mocks.enqueue.mock.calls[0][2]).toBe('peer:current')
  })

  it('does not acknowledge capture consumption when upload queue acceptance fails', async () => {
    const revokeUrl = vi.spyOn(URL, 'revokeObjectURL')
    const chatItems = ref<any[]>([])
    const upload = useChatUpload(
      computed(() => 'peer:recipient'),
      computed(() => ''),
      '/captured/app',
      mocks.scrollBottom,
      ref(''),
      chatItems
    )
    mocks.enqueue.mockRejectedValueOnce(new Error('upload queue unavailable'))

    await expect(upload.doUploadImages([new File(['png'], 'capture.png', { type: 'image/png' })])).rejects.toThrow('upload queue unavailable')
    expect(chatItems.value).toEqual([])
    expect(revokeUrl).toHaveBeenCalledOnce()
  })

  it('revokes every optimistic capture URL and clears sending state across 100 completed cycles', async () => {
    let created = 0
    const createUrl = vi.spyOn(URL, 'createObjectURL').mockImplementation(() => `blob:capture-${created++}`)
    const revokeUrl = vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => undefined)
    const chatItems = ref<any[]>([])
    const upload = useChatUpload(
      computed(() => 'peer:recipient'),
      computed(() => ''),
      '/captured/app',
      mocks.scrollBottom,
      ref(''),
      chatItems
    )

    for (let index = 0; index < 100; index += 1) {
      await upload.doUploadImages([new File([`png-${index}`], `capture-${index}.png`, { type: 'image/png' })])
      const tempId = mocks.enqueue.mock.calls[index][0].id as string
      expect(upload.sendingText(tempId)).toBe('sending')
    }

    expect(createUrl).toHaveBeenCalledTimes(100)
    expect(revokeUrl.mock.calls.map(([url]) => url)).toEqual(Array.from({ length: 100 }, (_, index) => `blob:capture-${index}`))
    expect(chatItems.value).toHaveLength(100)
  })
})
