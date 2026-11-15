import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { createSSRApp, h, ref, KeepAlive } from 'vue'
import { renderView, flush } from '../helpers/vue-node-renderer'
import { renderToString } from 'vue/server-renderer'

const mock = vi.hoisted(() => ({ query: vi.fn(), invoke: vi.fn(), values: vi.fn(), localPorts: vi.fn(), assign: vi.fn(), toast: vi.fn(), emit: vi.fn(), local: false, listeners: new Map<string, Set<(data: any) => void>>() }))
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

vi.mock('@/lib/api/subscription-client', () => ({ graphqlSubscriptions: {
  subscribe(field: string, callback: (data: any) => void) {
    const listeners = mock.listeners.get(field) ?? new Set()
    mock.listeners.set(field, listeners); listeners.add(callback)
    return () => listeners.delete(callback)
  },
  onAttached(callback: () => void) {
    const listeners = mock.listeners.get('attached') ?? new Set()
    mock.listeners.set('attached', listeners); listeners.add(callback)
    return () => listeners.delete(callback)
  },
} }))

import { useHttpServer } from '@/views/home/remote-access/use-http-server'
import { httpServerQuery, restartHttpServerMutation } from '@/lib/api/http-server'

const state = { revision: 1, restarting: false, httpPort: 8180, httpsPort: 8543, error: '' }
const snapshot = { state, addresses: ['192.0.2.8'], onlineClients: ['a', 'b'], remoteAccess: true, mdnsHostname: 'fixture.local' }
beforeEach(() => {
  vi.clearAllMocks()
  mock.listeners.clear()
  mock.local = false
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

it('does not query on a timer and applies pushed server snapshots', async () => {
  const value = await server()
  await vi.advanceTimersByTimeAsync(10_000)
  expect(mock.query).toHaveBeenCalledTimes(1)
  for (const callback of mock.listeners.get('httpServerChanged') ?? []) callback({ ...snapshot, onlineClients: [], state: { ...state, httpPort: 8280 } })
  expect(value.count.value).toBe(0)
  expect(value.httpAddresses.value).toEqual(['http://192.0.2.8:8280'])
  expect(mock.query).toHaveBeenCalledTimes(1)
})

it('detaches while Home is cached, refreshes on return and ignores late reads', async () => {
  const visible = ref(true)
  let value!: ReturnType<typeof useHttpServer>
  const Home = { setup() { value = useHttpServer(); return () => h('div') } }
  const view = renderView({ setup: () => () => h(KeepAlive, null, { default: () => visible.value ? h(Home) : h('div') }) })
  try {
    await flush()
    expect(mock.listeners.get('httpServerChanged')?.size).toBe(1)
    let finish!: (value: any) => void
    mock.query.mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))
    for (const callback of mock.listeners.get('attached') ?? []) callback()
    visible.value = false; await flush()
    expect(mock.listeners.get('httpServerChanged')?.size).toBe(0)
    expect(mock.listeners.get('attached')?.size).toBe(0)
    const calls = mock.query.mock.calls.length
    await vi.advanceTimersByTimeAsync(10_000)
    expect(mock.query).toHaveBeenCalledTimes(calls)
    visible.value = true; await flush()
    expect(mock.query).toHaveBeenCalledTimes(calls + 1)
    expect(mock.listeners.get('httpServerChanged')?.size).toBe(1)
    finish({ data: { httpServer: { ...snapshot, state: { ...state, httpPort: 9999 } } } }); await flush()
    expect(value.httpPort.value).toBe(8180)
  } finally { view.unmount() }
  expect(mock.listeners.get('httpServerChanged')?.size).toBe(0)
})

it('releases local network and client callbacks while Home is cached', async () => {
  mock.local = true
  mock.invoke.mockImplementation(async (method?: string) => {
    if (method === 'native_register_online_clients') return 1
    if (method === 'native_register_network_changes') return 2
    if (method === undefined) return ['192.0.2.8']
  })
  const visible = ref(true)
  const Home = { setup() { useHttpServer(); return () => h('div') } }
  const view = renderView({ setup: () => () => h(KeepAlive, null, { default: () => visible.value ? h(Home) : h('div') }) })
  try {
    await flush()
    const oldNetwork = mock.invoke.mock.calls.find(call => call[0] === 'native_register_network_changes')![1].channel
    visible.value = false; await flush()
    expect(mock.invoke).toHaveBeenCalledWith('native_unregister_network_changes', { id: 2 })
    expect(mock.invoke).toHaveBeenCalledWith('native_unregister_online_clients', { id: 1 })
    visible.value = true; await flush()
    const calls = mock.invoke.mock.calls.length
    oldNetwork.onmessage({}); await flush()
    expect(mock.invoke).toHaveBeenCalledTimes(calls)
    expect(mock.invoke.mock.calls.filter(call => call[0] === 'native_register_network_changes')).toHaveLength(2)
  } finally { view.unmount(); mock.local = false }
})

it('refreshes once after reconnect and keeps a newer pushed snapshot over a pending read', async () => {
  const value = await server()
  let finish!: (value: any) => void
  mock.query.mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))
  for (const callback of mock.listeners.get('attached') ?? []) callback()
  expect(mock.query).toHaveBeenCalledTimes(2)
  for (const callback of mock.listeners.get('httpServerChanged') ?? []) callback({ ...snapshot, state: { ...state, httpPort: 8280 } })
  finish({ data: { httpServer: snapshot } }); await flush()
  expect(value.httpPort.value).toBe(8280)
  expect(mock.listeners.get('httpServerChanged')?.size).toBe(1)
})

it('recovers an initial read failure from the next pushed snapshot without a timer retry', async () => {
  vi.spyOn(console, 'error').mockImplementation(() => {})
  mock.query.mockRejectedValueOnce(new Error('fixture disconnected'))
  const value = await server()
  expect(value.addressError.value).toBe(true)
  expect(value.clientsFailed.value).toBe(true)
  await vi.advanceTimersByTimeAsync(10_000)
  expect(mock.query).toHaveBeenCalledTimes(1)
  for (const callback of mock.listeners.get('httpServerChanged') ?? []) callback(snapshot)
  expect(value.addressError.value).toBe(false)
  expect(value.clientsFailed.value).toBe(false)
  expect(value.count.value).toBe(2)
})
