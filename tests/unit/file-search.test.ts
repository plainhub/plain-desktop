import { beforeEach, describe, expect, it, vi } from 'vitest'

const connection = vi.hoisted(() => ({ send: vi.fn(), close: vi.fn(), callbacks: undefined as any }))
vi.mock('@/lib/api/subscription-connection', () => ({
  openSubscriptionConnection: vi.fn((callbacks: unknown) => { connection.callbacks = callbacks; return connection }),
}))
vi.mock('@/lib/api/query', () => ({ fileSearchResultsGQL: 'subscription fileSearchResults { fileSearchResults }' }))
import { searchFiles } from '@/lib/api/file-search'

const variables = { root: '/documents', query: 'needle', offset: 0, limit: 20, sortBy: 'NAME_ASC' as const }
const page = { items: [], offset: 0, count: 0, complete: false }

beforeEach(() => { vi.clearAllMocks() })
describe('directory search stream', () => {
  it('sends search arguments, delivers partial pages and closes after the final page', () => {
    const next = vi.fn(), failed = vi.fn()
    searchFiles(variables, next, failed)
    connection.callbacks.opened()
    expect(connection.send.mock.calls[0][0].payload.variables).toEqual(variables)
    connection.callbacks.message({ type: 'next', id: 'search', payload: { extensions: { subscriptionReady: true } } })
    expect(next).not.toHaveBeenCalled()
    connection.callbacks.message({ type: 'next', id: 'search', payload: { data: { fileSearchResults: page } } })
    expect(next).toHaveBeenCalledWith(page)
    expect(connection.close).not.toHaveBeenCalled()
    connection.callbacks.message({ type: 'next', id: 'search', payload: { data: { fileSearchResults: { ...page, complete: true } } } })
    expect(connection.close).toHaveBeenCalledOnce()
    expect(failed).not.toHaveBeenCalled()
  })
  it('cancels and ignores late results', () => {
    const next = vi.fn(), failed = vi.fn()
    const cancel = searchFiles(variables, next, failed)
    cancel()
    connection.callbacks.opened()
    connection.callbacks.message({ type: 'next', id: 'search', payload: { data: { fileSearchResults: page } } })
    connection.callbacks.dropped(new Error('late'))
    expect(connection.close).toHaveBeenCalledOnce()
    expect(connection.send).not.toHaveBeenCalled()
    expect(next).not.toHaveBeenCalled()
    expect(failed).not.toHaveBeenCalled()
  })
  it('reports permission errors and premature completion without presenting complete results', () => {
    for (const message of [
      { type: 'error', id: 'search', payload: [{ message: 'no_permission' }] },
      { type: 'complete', id: 'search' },
    ]) {
      const failed = vi.fn()
      searchFiles(variables, vi.fn(), failed)
      connection.callbacks.message(message)
      expect(failed).toHaveBeenCalledOnce()
    }
  })
})

it('assembles bounded batches and replaces the previous snapshot only when ready', () => {
  const next = vi.fn(), failed = vi.fn()
  searchFiles(variables, next, failed)
  function send(items: unknown[], offset: number, count: number, complete = false) {
    connection.callbacks.message({ type: 'next', id: 'search', payload: { data: { fileSearchResults: { items, offset, count, complete } } } })
  }
  send([{ path: '/b' }], 0, 1)
  expect(next).toHaveBeenCalledTimes(1)
  send([{ path: '/a' }], 0, 3)
  send([{ path: '/b' }], 1, 3)
  expect(next).toHaveBeenCalledTimes(1)
  send([{ path: '/c' }], 2, 3, true)
  expect(next.mock.calls[1][0].items.map((item: any) => item.path)).toEqual(['/a', '/b', '/c'])
  expect(connection.close).toHaveBeenCalledOnce()
  expect(failed).not.toHaveBeenCalled()
})

it('rejects missing batches and ignores buffered results after cancellation', () => {
  const next = vi.fn(), failed = vi.fn()
  const cancel = searchFiles(variables, next, failed)
  connection.callbacks.message({ type: 'next', id: 'search', payload: { data: { fileSearchResults: { items: [], offset: 1, count: 3, complete: false } } } })
  expect(failed).toHaveBeenCalledOnce()
  cancel()
  connection.callbacks.message({ type: 'next', id: 'search', payload: { data: { fileSearchResults: { items: [], offset: 0, count: 0, complete: true } } } })
  expect(next).not.toHaveBeenCalled()
})
