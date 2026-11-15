import { beforeEach, expect, it, vi } from 'vitest'
import { invoke } from '@/lib/api/native-invoke'
import { isLocalMode } from '@/lib/device/local-mode'
import { subscribeOnlineClients } from '@/lib/device/sessions'

vi.mock('@/lib/api/native-invoke', () => ({ invoke: vi.fn(), Channel: class { onmessage?: (value: unknown) => void } }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: vi.fn(() => true) }))
beforeEach(() => { vi.resetAllMocks(); vi.mocked(isLocalMode).mockReturnValue(true) })

it('does not read local online status from a remote view', async () => {
  vi.mocked(isLocalMode).mockReturnValue(false)
  await expect(subscribeOnlineClients(vi.fn())).rejects.toThrow('native_only')
  expect(invoke).not.toHaveBeenCalled()
})

it('delivers online snapshots and unregisters the exact callback', async () => {
  let channel!: { onmessage: (value: { clientIds: string[] }) => void }
  vi.mocked(invoke).mockImplementationOnce(async (_, args: any) => {
    channel = args.channel
    channel.onmessage({ clientIds: ['706c6169-1dfd-8b72-82bb-3ad00a9928ca'] })
    return 24 as any
  })
  const receive = vi.fn()
  const close = await subscribeOnlineClients(receive)
  expect(receive).toHaveBeenLastCalledWith(['706c6169-1dfd-8b72-82bb-3ad00a9928ca'])
  channel.onmessage({ clientIds: [] })
  expect(receive).toHaveBeenLastCalledWith([])
  await close()
  expect(invoke).toHaveBeenLastCalledWith('native_unregister_online_clients', { id: 24 })
})
