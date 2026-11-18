import { effectScope, ref } from 'vue'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { useClipboardMasterSwitch } from '@/views/clipboard/master-switch'

const mock = vi.hoisted(() => ({ request: vi.fn(), subscribe: vi.fn(), local: true }))
vi.mock('@/lib/api/gql-client', () => ({ bindGraphqlClient: () => mock.request }))
vi.mock('@/lib/api/mutation', () => ({ setPrefGQL: 'setPref' }))
vi.mock('@/lib/api/subscription-client', () => ({ graphqlSubscriptions: { subscribe: mock.subscribe } }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => mock.local }))
vi.mock('@/lib/prefs/core', () => ({ preferencesQuery: 'prefs' }))

let scope = effectScope()
const visible = ref(false)
beforeEach(() => {
  vi.resetAllMocks()
  scope = effectScope()
  visible.value = false
  mock.local = true
})
afterEach(() => scope.stop())

function mount() {
  return scope.run(() => useClipboardMasterSwitch(() => visible.value))!
}

const permissions = (api_permissions: string[]) => ({ data: { prefs: { api_permissions } } })

it('patches only clipboard in an active local window, and waits for authoritative readback', async () => {
  let finish!: (value: unknown) => void
  const stop = vi.fn()
  mock.subscribe.mockReturnValue(stop)
  mock.request.mockResolvedValueOnce(permissions(['CLIPBOARD', 'FILES']))
    .mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))
    .mockResolvedValueOnce(permissions(['FILES']))
  const control = mount()
  expect(mock.request).not.toHaveBeenCalled()
  visible.value = true
  await vi.waitFor(() => expect(control.enabled.value).toBe(true))
  const saving = control.setEnabled(false)
  expect(control.enabled.value).toBe(true)
  expect(control.saving.value).toBe(true)
  await control.setEnabled(false)
  expect(mock.request.mock.calls).toEqual([
    ['prefs', { keys: ['api_permissions'] }],
    ['setPref', { key: 'api_permissions', value: { CLIPBOARD: false } }],
  ])
  finish({ data: { setPref: true } })
  await saving
  expect(control.enabled.value).toBe(false)
  expect(control.saving.value).toBe(false)
  expect(mock.request).toHaveBeenLastCalledWith('prefs', { keys: ['api_permissions'] })
  visible.value = false
  await vi.waitFor(() => expect(stop).toHaveBeenCalledOnce())
})

it('never writes a managed remote window and preserves last known state on a refused write', async () => {
  mock.local = false
  visible.value = true
  await mount().setEnabled(true)
  expect(mock.request).not.toHaveBeenCalled()
  mock.local = true
  mock.subscribe.mockReturnValue(vi.fn())
  mock.request.mockResolvedValueOnce(permissions(['FILES'])).mockResolvedValueOnce({ data: { setPref: false } })
  const control = mount()
  await vi.waitFor(() => expect(control.enabled.value).toBe(false))
  await expect(control.setEnabled(true)).rejects.toThrow('refused')
  expect(control.enabled.value).toBe(false)
})
