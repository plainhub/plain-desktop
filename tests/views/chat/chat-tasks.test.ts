import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { IChatItem } from '@/lib/interfaces'
import type { IUploadItem } from '@/stores/temp'

const mocks = vi.hoisted(() => ({
  gqlFetch: vi.fn(),
  upload: vi.fn(),
}))

vi.mock('@/lib/api/gql-client', () => ({
  gqlFetch: mocks.gqlFetch,
  GqlError: class GqlError extends Error {},
}))
vi.mock('@/lib/upload/upload', () => ({ upload: mocks.upload }))

import { getUploadQueueStatus } from '@/lib/upload/upload-queue'
import { useTasks } from '@/views/chat/hooks/chat'

function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (error: unknown) => void
  const promise = new Promise<T>((res, rej) => {
    resolve = res
    reject = rej
  })
  return { promise, resolve, reject }
}

function uploadItem(id: string): IUploadItem {
  return {
    id,
    dir: '/chat',
    fileName: '',
    file: new File([id], `${id}.png`, { type: 'image/png' }),
    uploadedSize: 0,
    status: 'pending',
    error: '',
    isAppFile: true,
  }
}

function chatItem(uploads: IUploadItem[]): IChatItem {
  const value = {
    items: uploads.map((upload) => ({
      uri: upload.file.name,
      size: upload.file.size,
      duration: 0,
      width: 1,
      height: 1,
    })),
  }
  return {
    id: 'new-message',
    fromId: 'me',
    toId: 'peer:recipient',
    channelId: '',
    createdAt: new Date(0).toISOString(),
    content: JSON.stringify({ type: 'images', value }),
    _content: { type: 'images', value },
    __typename: 'ChatItem',
  } as IChatItem
}

describe('chat upload task transaction', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mocks.gqlFetch.mockResolvedValue({ data: { sendChatItem: { id: 'sent-message' } } })
  })

  it('does not resolve or send the chat item until every upload succeeds', async () => {
    const uploadGate = deferred<void>()
    mocks.upload.mockImplementation(async (upload: IUploadItem) => {
      await uploadGate.promise
      upload.fileHash = `hash-${upload.id}`
      upload.status = 'done'
      return { fileName: upload.fileHash }
    })
    const uploads = [uploadItem('first'), uploadItem('second')]
    const onSent = vi.fn()
    let settled = false

    const completion = useTasks()
      .enqueue(chatItem(uploads), uploads, 'peer:recipient', onSent)
      .finally(() => {
        settled = true
      })
    await Promise.resolve()

    expect(settled).toBe(false)
    expect(mocks.gqlFetch).not.toHaveBeenCalled()

    uploadGate.resolve()
    await completion

    expect(mocks.gqlFetch).toHaveBeenCalledOnce()
    expect(JSON.parse(mocks.gqlFetch.mock.calls[0][1].content).value.items).toMatchObject([
      { uri: 'fid:hash-first', fileName: 'first.png' },
      { uri: 'fid:hash-second', fileName: 'second.png' },
    ])
    expect(onSent).toHaveBeenCalledWith({ id: 'sent-message' })
    expect(getUploadQueueStatus().total).toBe(0)
  })

  it('rejects upload failure without attempting sendChatItem', async () => {
    mocks.upload.mockImplementation(async (upload: IUploadItem) => {
      upload.status = 'error'
      upload.error = 'network unavailable'
      return { error: upload.error }
    })
    const uploads = [uploadItem('failed')]

    await expect(useTasks().enqueue(chatItem(uploads), uploads, 'peer:recipient')).rejects.toThrow('network unavailable')

    expect(mocks.gqlFetch).not.toHaveBeenCalled()
    expect(getUploadQueueStatus().total).toBe(0)
  })

  it('rejects message mutation failure after successful uploads', async () => {
    mocks.upload.mockImplementation(async (upload: IUploadItem) => {
      upload.fileHash = `hash-${upload.id}`
      upload.status = 'done'
      return { fileName: upload.fileHash }
    })
    mocks.gqlFetch.mockRejectedValue(new Error('send mutation failed'))
    const uploads = [uploadItem('mutation-failure')]

    await expect(useTasks().enqueue(chatItem(uploads), uploads, 'peer:recipient')).rejects.toThrow('send mutation failed')

    expect(getUploadQueueStatus().total).toBe(0)
  })

  it('rejects a mutation response that does not confirm a sent message', async () => {
    mocks.upload.mockImplementation(async (upload: IUploadItem) => {
      upload.fileHash = `hash-${upload.id}`
      upload.status = 'done'
      return { fileName: upload.fileHash }
    })
    mocks.gqlFetch.mockResolvedValue({ data: { sendChatItem: null } })
    const uploads = [uploadItem('missing-message')]

    await expect(useTasks().enqueue(chatItem(uploads), uploads, 'peer:recipient')).rejects.toThrow('sendChatItem returned no message')

    expect(getUploadQueueStatus().total).toBe(0)
  })

  it('lets a separate chat consumer cancel the owning in-flight transaction', async () => {
    const uploadGate = deferred<void>()
    mocks.upload.mockImplementation(async (upload: IUploadItem) => {
      await uploadGate.promise
      upload.fileHash = `hash-${upload.id}`
      upload.status = 'done'
      return { fileName: upload.fileHash }
    })
    const uploads = [uploadItem('cancelled')]
    const item = chatItem(uploads)
    const completion = useTasks().enqueue(item, uploads, 'peer:recipient')
    await Promise.resolve()

    useTasks().cancel(item.id)

    await expect(completion).rejects.toThrow('Upload canceled')
    expect(mocks.gqlFetch).not.toHaveBeenCalled()
    expect(getUploadQueueStatus().total).toBe(0)
    uploadGate.resolve()
  })
})
