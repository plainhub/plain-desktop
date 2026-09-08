import { describe, expect, it, vi, beforeEach } from 'vitest'

const gqlFetchMock = vi.fn()
vi.mock('@/lib/api/gql-client', () => {
  class GqlError extends Error {}
  return { gqlFetch: (...args: any[]) => gqlFetchMock(...args), GqlError }
})

import { fetchLatestChatContent } from '@/views/chat/hooks/forward-content'

beforeEach(() => {
  gqlFetchMock.mockReset()
})

describe('fetchLatestChatContent', () => {
  it('uses the single-item query result when the server provides it', async () => {
    gqlFetchMock.mockResolvedValue({ data: { chatItem: { id: 'm1', content: '{"fresh":true}' } } })

    await expect(fetchLatestChatContent('m1', 'peer:p1')).resolves.toBe('{"fresh":true}')
    expect(gqlFetchMock).toHaveBeenCalledTimes(1)
    expect(gqlFetchMock.mock.calls[0][1]).toEqual({ id: 'm1' })
  })

  it('falls back to the conversation query when the server has no chatItem field', async () => {
    gqlFetchMock.mockResolvedValueOnce({ data: {}, errors: [{ message: 'Cannot query field chatItem' }] })
    gqlFetchMock.mockResolvedValueOnce({ data: { chatItems: [{ id: 'other', content: 'x' }, { id: 'm1', content: 'fresh' }] } })

    await expect(fetchLatestChatContent('m1', 'peer:p1')).resolves.toBe('fresh')
    expect(gqlFetchMock).toHaveBeenCalledTimes(2)
    expect(gqlFetchMock.mock.calls[1][1]).toEqual({ id: 'peer:p1' })
  })

  it('returns null when the message is absent from the conversation result', async () => {
    gqlFetchMock.mockResolvedValueOnce({ data: {} })
    gqlFetchMock.mockResolvedValueOnce({ data: { chatItems: [{ id: 'other', content: 'x' }] } })

    await expect(fetchLatestChatContent('m1', 'peer:p1')).resolves.toBeNull()
  })

  it('returns null when both queries throw', async () => {
    gqlFetchMock.mockRejectedValue(new Error('network down'))

    await expect(fetchLatestChatContent('m1', 'peer:p1')).resolves.toBeNull()
    expect(gqlFetchMock).toHaveBeenCalledTimes(1)
  })

  it('does not run the fallback when the single query succeeds', async () => {
    gqlFetchMock.mockResolvedValue({ data: { chatItem: { id: 'm1', content: 'ok' } } })

    await fetchLatestChatContent('m1', 'peer:p1')
    expect(gqlFetchMock).toHaveBeenCalledTimes(1)
  })
})
