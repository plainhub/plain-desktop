import { createPinia, setActivePinia } from 'pinia'
import { effectScope } from 'vue'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { useClipboardData } from '@/views/clipboard/clipboard'
import { setClipboardGQL } from '@/lib/api/mutation'
import { clipboardItemsGQL } from '@/lib/api/query'

const { gqlFetch, showActionError } = vi.hoisted(() => ({ gqlFetch: vi.fn(), showActionError: vi.fn() }))
vi.mock('@/lib/api/gql-client', () => ({ gqlFetch }))
vi.mock('@/lib/api/subscription-client', () => ({ graphqlSubscriptions: {} }))
vi.mock('@/hooks/show-action-error', () => ({ showActionError }))
vi.mock('@/stores/main', async () => {
  const { defineStore } = await import('pinia')
  return { useMainStore: defineStore('main', { state: () => ({ pageSize: 2 }) }) }
})
vi.mock('@/stores/temp', async () => {
  const { defineStore } = await import('pinia')
  return { useTempStore: defineStore('temp', { state: () => ({ app: { access: ['CLIPBOARD'] } }) }) }
})

const history = (id: string, count: number) => ({
  data: { clipboardItems: [{ id, text: id }], clipboardItemCount: count },
})
let scope: ReturnType<typeof effectScope>
beforeEach(() => {
  vi.resetAllMocks()
  setActivePinia(createPinia())
  scope = effectScope()
})
afterEach(() => {
  scope.stop()
  setActivePinia(undefined)
})

it('returns to page one and reloads rows and count after the phone saves the clipboard entry', async () => {
  const oldPage = Promise.withResolvers<ReturnType<typeof history>>()
  const sent = Promise.withResolvers<{ data: { setClipboard: boolean } }>()
  gqlFetch.mockResolvedValueOnce(history('first', 5)).mockReturnValueOnce(oldPage.promise)
    .mockReturnValueOnce(sent.promise).mockResolvedValueOnce(history('just sent', 6))
  const clipboard = scope.run(useClipboardData)!
  await Promise.resolve()
  clipboard.gotoPage(3)
  clipboard.clipText.value = 'just sent'
  const sending = clipboard.sendToPhone()
  expect(gqlFetch).toHaveBeenCalledTimes(3)
  expect(gqlFetch).toHaveBeenNthCalledWith(2, clipboardItemsGQL, { offset: 4, limit: 2, query: '' })
  expect(gqlFetch).toHaveBeenNthCalledWith(3, setClipboardGQL, { text: 'just sent' }, { dedupe: false })
  expect(clipboard.page.value).toBe(3)

  sent.resolve({ data: { setClipboard: true } })
  await sending
  expect(clipboard.page.value).toBe(1)
  expect(gqlFetch).toHaveBeenNthCalledWith(4, clipboardItemsGQL, { offset: 0, limit: 2, query: '' }, { fresh: true })
  expect(clipboard.items.value.map(item => item.id)).toEqual(['just sent'])
  expect(clipboard.total.value).toBe(6)
  expect(clipboard.clipText.value).toBe('')

  oldPage.resolve(history('stale', 5))
  await oldPage.promise
  expect(clipboard.items.value.map(item => item.id)).toEqual(['just sent'])
  expect(clipboard.total.value).toBe(6)
})

it.each([
  { data: { setClipboard: false } },
  { errors: [{ message: 'CLIPBOARD_SYNC_DISABLED' }] },
])('does not clear or reload when sending is unsuccessful: %j', async result => {
  gqlFetch.mockResolvedValueOnce(history('first', 5)).mockResolvedValueOnce(history('second page', 5))
    .mockResolvedValueOnce(result)
  const clipboard = scope.run(useClipboardData)!
  clipboard.gotoPage(2)
  clipboard.clipText.value = 'keep me'
  await clipboard.sendToPhone()
  expect(clipboard.clipText.value).toBe('keep me')
  expect(clipboard.page.value).toBe(2)
  expect(clipboard.items.value.map(item => item.id)).toEqual(['second page'])
  expect(gqlFetch).toHaveBeenCalledTimes(3)
})

it('leaves a newer draft intact and does not start a second write while sending', async () => {
  const sent = Promise.withResolvers<{ data: { setClipboard: boolean } }>()
  gqlFetch.mockResolvedValueOnce(history('first', 5)).mockReturnValueOnce(sent.promise)
    .mockResolvedValueOnce(history('previous draft', 6))
  const clipboard = scope.run(useClipboardData)!
  clipboard.clipText.value = 'previous draft'
  const sending = clipboard.sendToPhone()
  clipboard.clipText.value = 'next draft'
  clipboard.sendToPhone()
  expect(gqlFetch).toHaveBeenCalledTimes(2)

  sent.resolve({ data: { setClipboard: true } })
  await sending
  expect(clipboard.clipText.value).toBe('next draft')
  expect(clipboard.items.value.map(item => item.id)).toEqual(['previous draft'])
})
