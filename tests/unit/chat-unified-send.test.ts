import { expect, it, vi } from 'vitest'
import type { IChatItem } from '@/lib/interfaces'
import type { IUploadItem } from '@/stores/temp'

const io = vi.hoisted(() => ({ upload: vi.fn(), send: vi.fn(), remove: vi.fn() }))
vi.mock('@/lib/upload/upload-queue', () => ({ addUploadTaskAndWait: io.upload, removeUpload: io.remove }))
vi.mock('@/lib/api/gql-client', () => ({ gqlFetch: io.send }))
import { useTasks } from '@/views/chat/hooks/chat'
import { chatContentInput } from '@/lib/chat/chat-content'

it('sends text and ordered mixed uploaded references once, without any local preview data', async () => {
  const types = ['FILE', 'IMAGE', 'FILE'] as const
  const make = (i: number) => ({ id: `upload-${i}`, uri: `blob:fixture-${i}`, size: 9, durationMs: 0, width: 10, height: 20, summary: '', fileName: `file-${i}`, thumbnail: 'blob:temp' })
  const content = { __typename: 'ChatMessageContent', text: '资料和 **说明**', format: 'MARKDOWN', items: types.map((type, i) => ({ type, attachment: make(i) })), linkPreviews: [] } satisfies IChatItem['content']
  const item: IChatItem = { id: 'new_fixture', __typename: 'ChatItem', fromId: 'me', toId: 'peer:original', channelId: null, createdAt: '', content, deliveryResults: [] }
  const uploads = types.map((_, i) => ({ file: new File(['synthetic'], `file-${i}`), id: `upload-${i}` })) as IUploadItem[]
  let finishUpload!: () => void
  const ready = new Promise<void>(resolve => { finishUpload = resolve })
  io.upload.mockImplementation(async upload => {
    await ready
    upload.fileHash = `hash-${upload.id}`
  })
  io.send.mockResolvedValue({ data: { sendChatItems: [{ id: 'sent' }] } })
  const sent = vi.fn()
  const result = useTasks().enqueue(item, uploads, item.toId, sent)
  expect(io.send).not.toHaveBeenCalled()
  finishUpload()
  await result
  expect(io.send).toHaveBeenCalledOnce()
  const payload = io.send.mock.calls[0]![1]
  expect(payload).toMatchObject({
    targets: ['peer:original'],
    content: { message: { text: content.text, format: 'MARKDOWN', items: types.map((type, i) => ({ type, attachment: { uri: `fid:hash-upload-${i}` } })) } },
  })
  expect(JSON.stringify(payload)).not.toContain('blob:')
  expect(JSON.stringify(payload)).not.toContain('thumbnail')
  expect(sent).toHaveBeenCalledWith({ id: 'sent' })
  const plain = chatContentInput({ ...content, format: 'PLAIN', items: [] })
  expect(plain).toEqual({ message: { text: content.text, format: 'PLAIN', items: [] } })
})
