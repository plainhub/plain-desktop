import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { createSSRApp, h } from 'vue'
import { renderToString } from 'vue/server-renderer'
import { useRemoteAccess } from '@/views/home/remote-access/use-remote-access'
import * as prefs from '@/lib/prefs/core'

const mock = vi.hoisted(() => ({ write: vi.fn(), toast: vi.fn() }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('@/lib/api/gql-client', () => ({
  getGraphqlContextKey: () => 'local',
  gqlFetch: vi.fn(),
  bindGraphqlClient: () => mock.write,
}))
vi.mock('@/components/toaster', () => ({ default: mock.toast }))

beforeEach(() => {
  vi.resetAllMocks()
  prefs.applySnapshot({})
})
afterEach(() => vi.restoreAllMocks())

async function access(value?: boolean) {
  prefs.applySnapshot(value === undefined ? {} : { remote_access: value, mdns_hostname: 'device.local' })
  let state!: ReturnType<typeof useRemoteAccess>
  const app = createSSRApp({
    setup() {
      state = useRemoteAccess()
      return () => h('div')
    },
  })
  await renderToString(app)
  return state
}

it('loads the canonical setting and hostname without enabling access on mount', async () => {
  const state = await access(false)
  expect(state.enabled.value).toBe(false)
  expect(state.hostname.value).toBe('device.local')
  expect(mock.write).not.toHaveBeenCalled()
})

it('retains the Rust default and follows external preference changes', async () => {
  const state = await access()
  expect(state.enabled.value).toBe(true)
  prefs.applySnapshot({ remote_access: false, mdns_hostname: 'updated.local' })
  expect(state.enabled.value).toBe(false)
  expect(state.hostname.value).toBe('updated.local')
})

it('changes the visible switch only after the canonical write succeeds', async () => {
  const state = await access(true)
  let finish!: (value: unknown) => void
  mock.write.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        finish = resolve
      })
  )
  const pending = state.save(false)
  await Promise.resolve()
  expect(state.saving.value).toBe(true)
  expect(state.enabled.value).toBe(true)
  expect(mock.write.mock.calls[0][1]).toEqual({ key: 'remote_access', value: false })
  finish({ data: { result: true } })
  await pending
  expect(state.enabled.value).toBe(false)
  expect(state.saving.value).toBe(false)
})

it('preserves saved state and exposes write failures', async () => {
  vi.spyOn(console, 'error').mockImplementation(() => {})
  const state = await access(true)
  mock.write.mockRejectedValueOnce(new Error('denied'))
  await state.save(false)
  expect(state.enabled.value).toBe(true)
  expect(state.saving.value).toBe(false)
  expect(mock.toast).toHaveBeenCalledWith('denied', 'error')
})
