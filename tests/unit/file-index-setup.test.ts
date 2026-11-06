import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { reactive } from 'vue'

const mock = vi.hoisted(() => ({
  local: true,
  mac: true,
  fetch: vi.fn(),
  write: vi.fn(),
  modal: vi.fn(),
  store: {} as { app: { capabilities?: string[] } },
}))
vi.mock('@/stores/temp', () => ({ useTempStore: () => mock.store }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => mock.local }))
vi.mock('@/lib/platform', () => ({ isMacPlatform: () => mock.mac }))
vi.mock('@/lib/api/gql-client', () => ({
  getGraphqlContextKey: () => 'local',
  gqlFetch: mock.fetch,
  bindGraphqlClient: () => mock.fetch,
}))
vi.mock('@/components/modal', () => ({ pushModal: mock.modal }))
vi.mock('@/components/FileIndexSetupDialog.vue', () => ({ default: {} }))

beforeEach(() => {
  vi.resetModules()
  vi.resetAllMocks()
  mock.local = true
  mock.mac = true
  mock.store = reactive({ app: { capabilities: ['MEDIA_SCAN'] } })
})

afterEach(() => vi.restoreAllMocks())

it('shows the explanation without writing configuration or starting another scan', async () => {
  const { useFileIndexSetup } = await import('@/hooks/file-index-setup')
  const setup = useFileIndexSetup()
  expect(setup.eligible.value).toBe(true)
  await setup.show()
  expect(mock.fetch).not.toHaveBeenCalled()
  expect(mock.write).not.toHaveBeenCalled()
})

it.each(['RUNNING', 'PAUSED'])('preserves an existing %s scan when continuing', async (state) => {
  const { useFileIndexSetup } = await import('@/hooks/file-index-setup')
  mock.fetch.mockResolvedValue({ data: { fileIndexVolumes: [{ id: 'root', path: '/', indexMode: 'BLOCKLIST' }], scanProgress: { state } } })
  await useFileIndexSetup().show()
  await mock.modal.mock.calls[0][1].onContinue()
  expect(mock.fetch).toHaveBeenCalledOnce()
  expect(mock.write).not.toHaveBeenCalled()
})

it.each(['IDLE', 'STOPPED', 'FAILED'])('rebuilds the existing volume scope from %s without preference writes', async (state) => {
  const { useFileIndexSetup } = await import('@/hooks/file-index-setup')
  mock.fetch
    .mockResolvedValueOnce({ data: { fileIndexVolumes: [{ id: 'root', path: '/', indexMode: 'BLOCKLIST' }], scanProgress: { state } } })
    .mockResolvedValueOnce({ data: { rebuildFileIndexVolume: true } })
  await useFileIndexSetup().show()
  await mock.modal.mock.calls[0][1].onContinue()
  expect(mock.fetch).toHaveBeenLastCalledWith(expect.stringContaining('rebuildFileIndexVolume'), { id: 'root' })
  expect(mock.write).not.toHaveBeenCalled()
})

it('keeps continuation errors visible to the explanation dialog', async () => {
  const { useFileIndexSetup } = await import('@/hooks/file-index-setup')
  mock.fetch.mockResolvedValue({ errors: [{ message: 'unavailable' }] })
  await useFileIndexSetup().show()
  await expect(mock.modal.mock.calls[0][1].onContinue()).rejects.toThrow('unavailable')
  expect(mock.write).not.toHaveBeenCalled()
})

it('limits the explanation to local macOS hosts with media scanning', async () => {
  const { useFileIndexSetup } = await import('@/hooks/file-index-setup')
  const setup = useFileIndexSetup()
  mock.store.app.capabilities = []
  expect(setup.eligible.value).toBe(false)
  mock.store.app.capabilities = ['MEDIA_SCAN']
  expect(setup.eligible.value).toBe(true)
  mock.mac = false
  expect(useFileIndexSetup().eligible.value).toBe(false)
  mock.mac = true
  mock.local = false
  expect(useFileIndexSetup().eligible.value).toBe(false)
})

it('configures the primary volume with the default blocklist only after continuation', async () => {
  const { useFileIndexSetup } = await import('@/hooks/file-index-setup')
  const root = { id: 'root', path: '/', isAvailable: true, indexMode: 'UNCONFIGURED', includedDirs: [], excludedDirs: [] }
  mock.fetch.mockResolvedValueOnce({ data: { fileIndexVolumes: [root], scanProgress: { state: 'IDLE' } } }).mockResolvedValueOnce({ data: { setFileIndexVolume: true } })
  const setup = useFileIndexSetup()
  await setup.show()
  expect(mock.fetch).not.toHaveBeenCalled()
  await mock.modal.mock.calls[0][1].onContinue()
  expect(mock.fetch.mock.calls[1][1]).toEqual({ id: 'root', indexMode: 'BLOCKLIST', includedDirs: [], excludedDirs: [] })
  expect(mock.write).not.toHaveBeenCalled()
})

it('reads the primary volume mode for onboarding without a preference sentinel', async () => {
  const { useFileIndexSetup } = await import('@/hooks/file-index-setup')
  mock.fetch
    .mockResolvedValueOnce({ data: { fileIndexVolumes: [{ path: '/', indexMode: 'UNCONFIGURED' }] } })
    .mockResolvedValueOnce({ data: { fileIndexVolumes: [{ path: '/', indexMode: 'DISABLED' }] } })
  const setup = useFileIndexSetup()
  expect(await setup.needsSetup()).toBe(true)
  expect(await setup.needsSetup()).toBe(false)
  expect(mock.write).not.toHaveBeenCalled()
})

it('remembers do not show again through the existing disabled volume mode', async () => {
  const { useFileIndexSetup } = await import('@/hooks/file-index-setup')
  const root = { id: 'root', path: '/', isAvailable: true, indexMode: 'UNCONFIGURED', includedDirs: ['/kept'], excludedDirs: ['/skipped'] }
  mock.fetch.mockResolvedValueOnce({ data: { fileIndexVolumes: [root], scanProgress: { state: 'IDLE' } } }).mockResolvedValueOnce({ data: { setFileIndexVolume: true } })
  await useFileIndexSetup().show()
  expect(mock.fetch).not.toHaveBeenCalled()
  await mock.modal.mock.calls[0][1].onDismiss()
  expect(mock.fetch.mock.calls[1][1]).toEqual({ id: 'root', indexMode: 'DISABLED', includedDirs: ['/kept'], excludedDirs: ['/skipped'] })
  expect(mock.fetch).toHaveBeenCalledTimes(2)
  expect(mock.write).not.toHaveBeenCalled()
})

it.each(['BLOCKLIST', 'ALLOWLIST', 'DISABLED'])('does not overwrite an already configured %s volume when dismissing', async (indexMode) => {
  const { useFileIndexSetup } = await import('@/hooks/file-index-setup')
  mock.fetch.mockResolvedValueOnce({ data: { fileIndexVolumes: [{ id: 'root', path: '/', indexMode }], scanProgress: { state: 'RUNNING' } } })
  await useFileIndexSetup().show()
  await mock.modal.mock.calls[0][1].onDismiss()
  expect(mock.fetch).toHaveBeenCalledOnce()
})
