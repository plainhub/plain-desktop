import { afterEach, expect, it, vi } from 'vitest'
import { useBrowserLogout } from '@/composables/useBrowserLogout'

const state = vi.hoisted(() => ({ fetch: vi.fn(), clear: vi.fn(), preferences: vi.fn(), reload: vi.fn() }))
vi.mock('@/lib/api/gql-client', () => ({ gqlFetch: state.fetch, GqlError: class extends Error {} }))
vi.mock('@/lib/device/current', () => ({ clearCurrentSession: state.clear }))
vi.mock('@/lib/prefs/browser', () => ({ clearForLogout: state.preferences }))

afterEach(() => { vi.clearAllMocks(); vi.unstubAllGlobals() })

it('revokes the authenticated server session before clearing local state and reloading', async () => {
  vi.stubGlobal('window', { location: { reload: state.reload } })
  let complete!: (result: unknown) => void
  state.fetch.mockImplementationOnce(() => new Promise(resolve => { complete = resolve }))
  const pending = useBrowserLogout()()
  expect(state.fetch.mock.calls[0][0]).toContain('logout')
  expect(state.clear).not.toHaveBeenCalled()
  expect(state.reload).not.toHaveBeenCalled()
  complete({ data: { logout: true } })
  await pending
  expect(state.clear).toHaveBeenCalledOnce()
  expect(state.preferences).toHaveBeenCalledExactlyOnceWith()
  expect(state.reload).toHaveBeenCalledOnce()
  expect(state.clear.mock.invocationCallOrder[0]).toBeLessThan(state.preferences.mock.invocationCallOrder[0])
  expect(state.preferences.mock.invocationCallOrder[0]).toBeLessThan(state.reload.mock.invocationCallOrder[0])
})

it('keeps the current login and page when the server rejects logout', async () => {
  vi.stubGlobal('window', { location: { reload: state.reload } })
  state.fetch.mockResolvedValueOnce({ errors: [{ message: 'logout rejected' }] })
  await useBrowserLogout()()
  expect(state.clear).not.toHaveBeenCalled()
  expect(state.preferences).not.toHaveBeenCalled()
  expect(state.reload).not.toHaveBeenCalled()
})
