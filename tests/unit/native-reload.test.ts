import { afterEach, beforeEach, expect, it, vi } from 'vitest'

const native = vi.hoisted(() => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke: native.invoke }))

beforeEach(() => { vi.resetModules(); vi.resetAllMocks() })
afterEach(() => vi.unstubAllGlobals())

it('preserves native request errors for their callers', async () => {
  const error = new Error('native failure')
  native.invoke.mockRejectedValue(error)
  const { invoke } = await import('@/lib/api/native-invoke')
  await expect(invoke('probe')).rejects.toBe(error)
})

it('closes streams and drains dispatched requests before accepting one full reload', async () => {
  const window = Object.assign(new EventTarget(), { location: { reload: vi.fn() } })
  vi.stubGlobal('window', window)
  let finishRequest!: () => void
  let finishClose!: () => void
  native.invoke.mockImplementation(command => {
    if (command === 'probe') return new Promise<void>(resolve => { finishRequest = resolve })
    if (command === 'close') return new Promise<void>(resolve => { finishClose = resolve })
    return Promise.resolve()
  })
  const { invoke, onNativeReload, installNativeReload } = await import('@/lib/api/native-invoke')
  installNativeReload()
  const pending = invoke('probe')
  const close = vi.fn(() => { void invoke('close') })
  onNativeReload(close)
  window.dispatchEvent(new Event('native-reload'))
  window.dispatchEvent(new Event('native-reload'))
  void invoke('new-document-work')
  expect(close).toHaveBeenCalledTimes(1)
  expect(native.invoke.mock.calls.map(([command]) => command)).toEqual(['probe', 'close'])
  expect(window.location.reload).not.toHaveBeenCalled()
  finishRequest()
  await pending
  expect(window.location.reload).not.toHaveBeenCalled()
  finishClose()
  await vi.waitFor(() => expect(window.location.reload).toHaveBeenCalledTimes(1))
  expect(native.invoke.mock.calls.map(([command]) => command)).toEqual(['probe', 'close', 'native_reload_ready'])
})
