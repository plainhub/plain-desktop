import { afterEach, beforeEach, expect, it, vi } from 'vitest'

const mock = vi.hoisted(() => ({
  invoke: vi.fn(),
  cleanup: undefined as (() => void) | undefined,
  channels: [] as Array<{ onmessage: (snapshot: { clientIds: string[] }) => void }>,
}))
vi.mock('vue', async (original) => ({
  ...(await original<typeof import('vue')>()),
  onUnmounted: (cleanup: () => void) => {
    mock.cleanup = cleanup
  },
}))
vi.mock('@tauri-apps/api/core', () => ({
  invoke: mock.invoke,
  Channel: class {
    onmessage!: (snapshot: { clientIds: string[] }) => void
    constructor() {
      mock.channels.push(this)
    }
  },
}))

beforeEach(() => {
  vi.resetModules()
  vi.resetAllMocks()
  mock.cleanup = undefined
  mock.channels = []
})
afterEach(() => vi.restoreAllMocks())

it('uses actual full snapshots including replay before registration completes', async () => {
  let finish!: (id: number) => void
  mock.invoke.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        finish = resolve
      })
  )
  const { useOnlineClients } = await import('@/views/home/remote-access/use-online-clients')
  const state = useOnlineClients()
  expect(state.count.value).toBe(null)
  mock.channels[0].onmessage({ clientIds: ['client-a', 'client-b'] })
  expect(state.count.value).toBe(2)
  finish(7)
  await Promise.resolve()
  mock.channels[0].onmessage({ clientIds: [] })
  expect(state.count.value).toBe(0)
  mock.invoke.mockResolvedValue(undefined)
  mock.cleanup?.()
  expect(mock.invoke).toHaveBeenLastCalledWith('native_unregister_online_clients', { id: 7 })
})

it('releases a late registration and ignores callbacks after unmount', async () => {
  let finish!: (id: number) => void
  mock.invoke.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        finish = resolve
      })
  )
  const { useOnlineClients } = await import('@/views/home/remote-access/use-online-clients')
  const state = useOnlineClients()
  mock.cleanup?.()
  mock.channels[0].onmessage({ clientIds: ['client-a'] })
  expect(state.count.value).toBe(null)
  mock.invoke.mockResolvedValue(undefined)
  finish(9)
  await Promise.resolve()
  expect(mock.invoke).toHaveBeenLastCalledWith('native_unregister_online_clients', { id: 9 })
})

it('reports unavailable status rather than inventing a zero count on registration failure', async () => {
  vi.spyOn(console, 'error').mockImplementation(() => {})
  mock.invoke.mockRejectedValueOnce(new Error('registration failed'))
  const { useOnlineClients } = await import('@/views/home/remote-access/use-online-clients')
  const state = useOnlineClients()
  await Promise.resolve()
  await Promise.resolve()
  expect(state.count.value).toBe(null)
  expect(state.failed.value).toBe(true)
})
