import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { createSSRApp, h, ref } from 'vue'
import { renderToString } from 'vue/server-renderer'

const mock = vi.hoisted(() => ({ query: vi.fn(), invoke: vi.fn(), values: vi.fn(), localPorts: vi.fn(), assign: vi.fn(), toast: vi.fn(), emit: vi.fn(), local: false }))
const appState = ref({ httpPort: 8080, httpsPort: 8443 })
vi.mock('@/lib/api/gql-client', () => ({ gqlFetchOp: (_name: string, ...args: unknown[]) => mock.query(...args) }))
vi.mock('@/lib/api/native-invoke', () => ({ invoke: mock.invoke, Channel: class {} }))
vi.mock('@/lib/api/local-server', () => ({ readLocalAddresses: mock.invoke, restartLocalServer: mock.invoke }))
vi.mock('@/lib/api/api', () => ({ setLocalServerPort: mock.localPorts, setLocalServerHttpsPort: mock.localPorts }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => mock.local }))
vi.mock('@/lib/prefs/core', () => ({ applyValues: mock.values }))
vi.mock('@/stores/temp', () => ({ useTempStore: () => ({ app: appState }) }))
vi.mock('pinia', () => ({ storeToRefs: (value: unknown) => value }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('@/components/toaster', () => ({ default: mock.toast }))
vi.mock('@/plugins/eventbus', () => ({ default: { emit: mock.emit } }))

import { useHttpServer } from '@/views/home/remote-access/use-http-server'
import { httpServerQuery, restartHttpServerMutation } from '@/lib/api/http-server'

const state = { revision: 1, restarting: false, httpPort: 8180, httpsPort: 8543, error: '' }
const snapshot = { state, addresses: ['192.0.2.8'], onlineClients: ['a', 'b'], remoteAccess: true, mdnsHostname: 'fixture.local' }
beforeEach(() => {
  vi.clearAllMocks()
  vi.useFakeTimers()
  vi.stubGlobal('__IS_TAURI__', false)
  vi.stubGlobal('window', { location: { href: 'http://fixture.local:8080/#/home', assign: mock.assign } })
  mock.query.mockImplementation(async (query: string) => ({ data: query === httpServerQuery ? { httpServer: snapshot } : { restartHttpServer: state } }))
})
afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); vi.restoreAllMocks() })

async function server() {
  let value!: ReturnType<typeof useHttpServer>
  await renderToString(createSSRApp({ setup() { value = useHttpServer(); return () => h('div') } }))
  await vi.advanceTimersByTimeAsync(0)
  return value
}

it('reads addresses, actual ports and online clients together without native commands', async () => {
  const value = await server()
  expect(value.httpAddresses.value).toEqual(['http://192.0.2.8:8180'])
  expect(value.httpsAddresses.value).toEqual(['https://192.0.2.8:8543'])
  expect(value.count.value).toBe(2)
  expect(mock.values).toHaveBeenCalledWith({ remote_access: true, mdns_hostname: 'fixture.local' })
  expect(mock.invoke).not.toHaveBeenCalled()
})

it('changes ports through the main schema and navigates to the actual completed listener', async () => {
  const value = await server()
  expect(await value.savePort('http', 8080)).toBe(true)
  expect(mock.query).toHaveBeenCalledWith(restartHttpServerMutation, { httpPort: 8080 })
  expect(mock.assign).toHaveBeenCalledWith('http://fixture.local:8180/#/home')
  expect(mock.localPorts).not.toHaveBeenCalled()
  expect(mock.toast).not.toHaveBeenCalled()
})

it('keeps the HTTPS address and route when an active TLS port changes', async () => {
  vi.stubGlobal('window', { location: { href: 'https://fixture.local:8443/#/home', assign: mock.assign } })
  const value = await server()
  expect(await value.savePort('https', 8543)).toBe(true)
  expect(mock.assign).toHaveBeenCalledWith('https://fixture.local:8543/#/home')
})

it('shows restart failures without navigating or changing local host ports', async () => {
  const value = await server()
  mock.query.mockResolvedValueOnce({ data: { restartHttpServer: { ...state, error: 'TLS identity invalid' } } })
  vi.spyOn(console, 'error').mockImplementation(() => {})
  expect(await value.savePort('http', 8180)).toBe(false)
  expect(mock.assign).not.toHaveBeenCalled()
  expect(mock.localPorts).not.toHaveBeenCalled()
  expect(mock.toast).toHaveBeenCalledWith('failed', 'error')
})

it('does not report an accepted but unfinished restart as saved', async () => {
  const value = await server()
  mock.query.mockResolvedValueOnce({ data: { restartHttpServer: { ...state, restarting: true } } })
  expect(await value.savePort('http', 8180)).toBe(false)
  expect(mock.assign).not.toHaveBeenCalled()
})

it('keeps visible addresses during background status refresh', async () => {
  const value = await server()
  mock.query.mockReturnValueOnce(new Promise(() => {}))
  await vi.advanceTimersByTimeAsync(2000)
  expect(value.loadingIps.value).toBe(false)
  expect(value.httpAddresses.value).toEqual(['http://192.0.2.8:8180'])
})

it('refreshes the card without navigating when the serving port stays the same', async () => {
  vi.stubGlobal('window', { location: { href: 'http://fixture.local:8180/#/home', assign: mock.assign } })
  const value = await server()
  expect(await value.savePort('https', 8543)).toBe(true)
  expect(mock.assign).not.toHaveBeenCalled()
  expect(mock.emit).toHaveBeenCalledWith('refetch_app')
})
