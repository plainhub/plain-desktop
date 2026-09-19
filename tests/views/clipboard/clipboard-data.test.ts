import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'

vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('@/components/toaster', () => ({ default: vi.fn() }))
vi.mock('@/plugins/eventbus', () => ({ default: { on: vi.fn(), off: vi.fn(), emit: vi.fn() } }))
vi.mock('@/lib/api/gql-client', () => ({
  gqlFetch: vi.fn(),
  GqlError: class GqlError extends Error {
    status?: number
    constructor(msg: string, status?: number) {
      super(msg)
      this.name = 'GqlError'
      this.status = status
    }
  },
}))

import toast from '@/components/toaster'
import emitter from '@/plugins/eventbus'
import { gqlFetch } from '@/lib/api/gql-client'
import { clipboardGQL } from '@/lib/api/query'
import { setClipboardGQL } from '@/lib/api/mutation'
import { useTempStore } from '@/stores/temp'
import { useClipboardData } from '@/views/clipboard/clipboard'

const mockGqlFetch = vi.mocked(gqlFetch)

const flush = () => new Promise((r) => setTimeout(r, 0))

const setAppSync = (enabled: boolean) => {
  useTempStore().app = { permissions: enabled ? ['CLIPBOARD'] : [] } as any
}

beforeEach(() => {
  setActivePinia(createPinia())
  vi.clearAllMocks()
})

describe('useClipboardData', () => {
  it('skips the list fetch while the phone switch is off — no toast', async () => {
    setAppSync(false)
    const api = useClipboardData()
    await flush()
    expect(mockGqlFetch).not.toHaveBeenCalled()
    expect(api.clipboardSync.value).toBe(false)
    expect(toast).not.toHaveBeenCalled()
  })

  it('loads the clipboard history at setup when sync is on', async () => {
    setAppSync(true)
    mockGqlFetch.mockResolvedValue({ data: { clipboard: [{ id: 'c1', text: 'hi' }], clipboardCount: 1 } })
    const api = useClipboardData()
    await flush()
    expect(mockGqlFetch).toHaveBeenCalledOnce()
    expect(mockGqlFetch.mock.calls[0][0]).toBe(clipboardGQL)
    expect(api.items.value).toHaveLength(1)
    expect(api.total.value).toBe(1)
  })

  it('open() re-fetches when enabled', async () => {
    setAppSync(true)
    mockGqlFetch.mockResolvedValue({ data: { clipboard: [], clipboardCount: 0 } })
    const api = useClipboardData()
    await flush()
    mockGqlFetch.mockClear()
    api.open()
    await flush()
    expect(mockGqlFetch).toHaveBeenCalledOnce()
  })

  it('open() emits refetch_app when disabled so a phone-side toggle is picked up', async () => {
    setAppSync(false)
    const api = useClipboardData()
    await flush()
    api.open()
    expect(emitter.emit).toHaveBeenCalledWith('refetch_app')
    expect(mockGqlFetch).not.toHaveBeenCalled()
  })

  it('reflects clipboard_sync_disabled from the list query without toasting', async () => {
    setAppSync(true)
    mockGqlFetch.mockResolvedValue({ data: null, errors: [{ message: 'clipboard_sync_disabled' }] })
    useClipboardData()
    await flush()
    expect(toast).not.toHaveBeenCalled()
  })

  it('still toasts unrelated list errors', async () => {
    setAppSync(true)
    mockGqlFetch.mockResolvedValue({ data: null, errors: [{ message: 'connection_timeout' }] })
    useClipboardData()
    await flush()
    expect(toast).toHaveBeenCalledWith('connection_timeout', 'error')
  })

  it('sendToPhone rejects empty text without calling the mutation', async () => {
    setAppSync(true)
    const api = useClipboardData()
    await flush()
    mockGqlFetch.mockClear()
    api.sendToPhone()
    await flush()
    expect(api.clipTextError.value).toBe(true)
    expect(mockGqlFetch).not.toHaveBeenCalled()
  })

  it('sendToPhone posts setClipboardGQL with the typed text', async () => {
    setAppSync(true)
    mockGqlFetch.mockResolvedValue({ data: { setClipboard: true } })
    const api = useClipboardData()
    await flush()
    mockGqlFetch.mockClear()
    api.clipText.value = 'hello'
    api.sendToPhone()
    await flush()
    expect(mockGqlFetch).toHaveBeenCalledWith(setClipboardGQL, { text: 'hello' }, { dedupe: false })
  })

  it('sendToPhone clears the input on success only', async () => {
    setAppSync(true)
    mockGqlFetch.mockResolvedValue({ data: null, errors: [{ message: 'network_error' }] })
    const api = useClipboardData()
    await flush()
    api.clipText.value = 'hello'
    api.sendToPhone()
    await flush()
    expect(api.clipText.value).toBe('hello')

    mockGqlFetch.mockResolvedValue({ data: { setClipboard: true } })
    api.sendToPhone()
    await flush()
    expect(api.clipText.value).toBe('')
  })
})
