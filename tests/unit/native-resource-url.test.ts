import { beforeEach, expect, it, vi } from 'vitest'

const fixture = vi.hoisted(() => ({ invoke: vi.fn(), local: true, scope: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke: fixture.invoke }))
vi.mock('../../src/lib/device/local-mode', () => ({ isLocalMode: () => fixture.local }))
vi.mock('../../src/lib/api/native-client', () => ({ nativeViewScope: fixture.scope }))

const resource = { id: 'handle', url: 'plain-resource://file/handle', name: 'fixture', mime: 'text/plain', size: 4 }

beforeEach(() => {
  vi.resetModules()
  vi.resetAllMocks()
  fixture.local = true
  fixture.scope.mockResolvedValue('document')
  fixture.invoke.mockImplementation(async (command: string) => command === 'native_resource_open' ? resource : true)
})

it('binds resource open and close to the same document scope', async () => {
  const { nativeResourceUrl, closeNativeResource } = await import('../../src/hooks/native-resource-url')
  expect(await nativeResourceUrl('/fixture')).toBe(resource.url)
  expect(await nativeResourceUrl('/fixture')).toBe(resource.url)
  expect(fixture.invoke).toHaveBeenCalledTimes(1)
  expect(fixture.invoke).toHaveBeenCalledWith('native_resource_open', { path: '/fixture', scope: 'document' })
  await closeNativeResource('/fixture')
  expect(fixture.invoke).toHaveBeenLastCalledWith('native_resource_close', { id: 'handle', scope: 'document' })
})

it('removes rejected pending opens so an explicit retry can succeed', async () => {
  fixture.invoke.mockRejectedValueOnce(new Error('file unavailable')).mockResolvedValue(resource)
  const { nativeResourceUrl } = await import('../../src/hooks/native-resource-url')
  await expect(nativeResourceUrl('/fixture')).rejects.toThrow('file unavailable')
  await expect(nativeResourceUrl('/fixture')).resolves.toBe(resource.url)
  expect(fixture.invoke).toHaveBeenCalledTimes(2)
})

it('closes a handle allocated after its consumer has gone away', async () => {
  let finish!: (value: typeof resource) => void
  fixture.invoke.mockImplementation((command: string) => command === 'native_resource_open'
    ? new Promise(resolve => { finish = resolve }) : Promise.resolve(true))
  const { nativeResourceUrl, closeNativeResource } = await import('../../src/hooks/native-resource-url')
  const opened = nativeResourceUrl('/fixture')
  const rejected = expect(opened).rejects.toThrow('resource_closed')
  await vi.waitFor(() => expect(finish).toBeDefined())
  await closeNativeResource('/fixture')
  finish(resource)
  await rejected
  expect(fixture.invoke).toHaveBeenLastCalledWith('native_resource_close', { id: 'handle', scope: 'document' })
})

it('does not register a local handle after the selected target changes', async () => {
  let finish!: (value: typeof resource) => void
  fixture.invoke.mockImplementation((command: string) => command === 'native_resource_open'
    ? new Promise(resolve => { finish = resolve }) : Promise.resolve(true))
  const { nativeResourceUrl } = await import('../../src/hooks/native-resource-url')
  const opened = nativeResourceUrl('/fixture')
  const rejected = expect(opened).rejects.toThrow('request_context_changed')
  await vi.waitFor(() => expect(finish).toBeDefined())
  fixture.local = false
  finish(resource)
  await rejected
  expect(fixture.invoke).toHaveBeenLastCalledWith('native_resource_close', { id: 'handle', scope: 'document' })
})

it('does not allocate after close while the scope handshake is pending', async () => {
  let finish!: (scope: string) => void
  fixture.scope.mockImplementation(() => new Promise(resolve => { finish = resolve }))
  const { nativeResourceUrl, closeNativeResource } = await import('../../src/hooks/native-resource-url')
  const opened = nativeResourceUrl('/fixture')
  const rejected = expect(opened).rejects.toThrow('resource_closed')
  await closeNativeResource('/fixture')
  finish('document')
  await rejected
  expect(fixture.invoke).not.toHaveBeenCalled()
})

it('recognizes only the native resource origins on each platform', async () => {
  const { isNativeResourceUrl, isExternalReference } = await import('../../src/hooks/native-resource-url')
  for (const url of ['plain-resource://file/handle', 'http://plain-resource.file/handle', 'https://plain-resource.file/handle']) {
    expect(isNativeResourceUrl(url)).toBe(true)
    expect(isExternalReference(url)).toBe(false)
  }
  for (const url of ['plain-resource://other/handle', 'http://plain-resource.file.example/handle', 'http://plain-resource.file:8080/handle', 'plain-resource://user@file/handle', 'not-a-url']) {
    expect(isNativeResourceUrl(url)).toBe(false)
  }
  expect(isExternalReference('https://example.test/image.png')).toBe(true)
})
