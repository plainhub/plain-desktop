import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { effectScope, nextTick, ref } from 'vue'
import { useGlobalSearch } from '@/hooks/global-search'
import { loadSearchClient } from '@/lib/api/global-search'
import { GqlError } from '@/lib/api/gql-client'
import { FileAction } from '@/lib/api/file-action'
import { appGQL, docsGQL, feedEntriesGQL, imagesGQL, notesGQL, videosGQL } from '@/lib/api/query'
import { filePathFromResourceId } from '@/lib/api/resource-link'
import { chachaDecrypt } from '@/lib/api/crypto'
import { loginPeers, type LoginPeer } from '@/lib/device/login-peers'
import type { IApp } from '@/lib/interfaces'
import type { GqlResultType } from '@/lib/api/graphql/operations'

const transport = vi.hoisted(() => ({ current: vi.fn(), local: vi.fn(), remote: vi.fn(), remoteId: '', authToken: '' }))
vi.mock('@/lib/api/gql-client', () => ({
  gqlFetch: transport.current, nativeGqlFetch: transport.local, encryptedGqlPost: transport.remote,
  GqlError: class extends Error { constructor(message: string, readonly status?: number) { super(message) } },
}))
vi.mock('@/lib/api/subscription-client', () => ({ graphqlSubscriptions: {} }))
vi.mock('@/lib/api/api', () => ({
  getApiBaseUrl: () => 'https://current.example.test',
  getApiHeaders: () => ({ 'c-id': 'search-client' }),
  getLocalServerPort: () => 8080,
  getProxyUrl: (path: string) => path,
  proxyUrlFor: (base: string, path: string) => base + path,
  deviceBaseUrl: (host: string) => `https://${host}`,
}))
vi.mock('@/lib/device/client-id', () => ({ getDesktopClientId: () => 'local', getRemoteClientId: () => transport.remoteId }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => false }))
vi.mock('@/lib/device/current', () => ({ getCurrentAuthToken: () => transport.authToken }))
vi.mock('@/lib/device/login-peers', async () => ({
  loginPeers: (await import('vue')).ref([]),
  peerHost: (peer: LoginPeer) => `${peer.ip}:${peer.port}`,
}))
vi.mock('@/components/toaster', () => ({ default: vi.fn(), toastWithAction: vi.fn() }))
vi.mock('@/lib/download-dir', () => ({ downloadToDir: vi.fn(), downloadAs: vi.fn(), revealInFolder: vi.fn() }))
vi.mock('@/hooks/native-resource-url', () => ({ nativeResourceUrl: vi.fn() }))

const peer = {
  id: 'peer', name: 'Archive', address: { baseUrl: 'https://archive.example.test', bestIp: '' }, token: 'peer-login', online: false,
} as LoginPeer
const current = { clientId: 'current', deviceName: 'Current' } as IApp
const key = new Uint8Array(32).fill(9)
const app = { deviceName: 'Archive', access: ['DOCS'], urlToken: btoa(String.fromCharCode(...key)) }
const doc = { id: 'doc', title: 'Search result.txt', path: '/docs/result.txt' }
let scope: ReturnType<typeof effectScope>

beforeEach(() => {
  vi.resetAllMocks()
  vi.stubGlobal('window', { fileIdMap: new Map() })
  vi.stubGlobal('__IS_TAURI__', false)
  transport.remoteId = ''
  transport.authToken = ''
  loginPeers.value = []
  scope = effectScope()
})

afterEach(() => {
  scope.stop()
  vi.unstubAllGlobals()
})

it('queries the authorized peer and seals preview URLs with that device’s session credential', async () => {
  transport.remote.mockResolvedValueOnce({ data: { app } }).mockResolvedValueOnce({ data: { items: [doc], total: 32 } })
  const client = await loadSearchClient({ id: peer.id, name: peer.name, target: 'peer', peer })
  expect(client.canSearch('images')).toBe(false)
  expect(client.canSearch('docs')).toBe(true)
  const { hits, total } = await client.page('docs', 'Trip: notes', 24)
  expect(total).toBe(32)
  expect(transport.current).not.toHaveBeenCalled()
  expect(transport.remote).toHaveBeenNthCalledWith(1, 'https://archive.example.test/graphql', 'peer-login', appGQL, undefined)
  expect(transport.remote).toHaveBeenNthCalledWith(2, 'https://archive.example.test/graphql', 'peer-login', docsGQL, {
    query: 'text:Trip:\\ notes trash:false', offset: 24, limit: 24, sortBy: 'DATE_DESC',
  })
  const hit = hits[0]
  if (hit.domain !== 'docs') throw new Error('Expected a document')
  const url = new URL(client.url(hit))
  expect(url.origin).toBe('https://archive.example.test')
  expect(url.searchParams.get('action')).toBe('preview')
  const envelope = url.searchParams.get('id')!
  expect(envelope).toMatch(/^c\.search-client\./)
  expect(filePathFromResourceId(key, envelope)).toBe(doc.path)
  expect(() => filePathFromResourceId(new Uint8Array(32).fill(7), envelope)).toThrow()
})

it('seals foreground thumbnail intent only for an explicitly submitted local video search', async () => {
  vi.stubGlobal('__IS_TAURI__', true)
  const hit = { domain: 'videos' as const, item: {
    ...doc, durationMs: 1000, size: 500, bucketId: 'bucket', createdAt: '', updatedAt: '', tags: [],
  } }
  const local = { id: 'local', name: 'Local', target: 'local' as const }
  transport.local.mockResolvedValue({ data: { app } })
  transport.remote.mockResolvedValue({ data: { app } })
  const passive = await loadSearchClient(local)
  const explicit = await loadSearchClient(local, true)
  const remote = await loadSearchClient({ id: peer.id, name: peer.name, target: 'peer', peer }, true)
  const readType = (url: string) => {
    const id = new URL(url).searchParams.get('id')!
    const [mode, ...parts] = id.split('.')
    const bytes = Uint8Array.from(atob(parts.pop()!.replace(/-/g, '+').replace(/_/g, '/')), c => c.charCodeAt(0))
    const aad = new TextEncoder().encode(`plain-fs-v1\0${mode}\0${parts.join('.')}`)
    return JSON.parse(chachaDecrypt(key, bytes, aad)).type
  }
  expect(readType(passive.thumbnailUrl(hit))).toBe('MEDIA')
  expect(readType(explicit.thumbnailUrl(hit))).toBe('FOREGROUND_MEDIA_THUMBNAIL')
  expect(readType(explicit.url(hit))).toBe('MEDIA')
  expect(readType(remote.thumbnailUrl(hit))).toBe('MEDIA')
  vi.stubGlobal('__IS_TAURI__', false)
  expect(readType(explicit.thumbnailUrl(hit))).toBe('MEDIA')
})

it('routes managed search through the current origin and reuses session-scoped thumbnail URLs', async () => {
  transport.authToken = 'browser-session'
  transport.remote.mockResolvedValue({ data: { app: { deviceName: 'Managed', access: ['DOCS'] } } })
  const client = await loadSearchClient({ id: 'managed', name: 'Managed', target: 'managed', online: true }, false, app.urlToken)
  expect(transport.remote).toHaveBeenCalledWith('https://current.example.test/proxy/managed/graphql', 'browser-session', '{ app { deviceName access } }', undefined)
  const hit = { domain: 'docs' as const, item: { ...doc, updatedAt: '2026-01-01' } as GqlResultType<'docsGQL'>['items'][number] }
  const thumbnail = client.thumbnailUrl(hit)
  expect(client.thumbnailUrl(hit)).toBe(thumbnail)
  const download = new URL(client.url(hit, FileAction.Download))
  expect(download.pathname).toBe('/proxy/managed/fs')
  expect(download.searchParams.get('action')).toBe('download')
  const [mode, owner, encoded] = download.searchParams.get('id')!.split('.')
  const bytes = Uint8Array.from(atob(encoded.replace(/-/g, '+').replace(/_/g, '/')), (char) => char.charCodeAt(0))
  const aad = (action: string) => new TextEncoder().encode(`plain-managed:managed:fs:${action}-v1\0${mode}\0${owner}`)
  expect(JSON.parse(chachaDecrypt(key, bytes, aad('download')))).toEqual({ type: 'MEDIA', id: doc.id })
  expect(() => chachaDecrypt(key, bytes, aad('preview'))).toThrow()
})

it('adds managed results without restarting the current search when device discovery finishes', async () => {
  transport.authToken = 'browser-session'
  const discovery = Promise.withResolvers<{ data: { peers: { id: string; name: string; managed: boolean; online: boolean }[] } }>()
  transport.current.mockImplementation((query: string) => {
    if (query === '{ peers { id name online managed } }') return discovery.promise
    return Promise.resolve({ data: query === appGQL ? { app } : { items: [doc], total: 1 } })
  })
  transport.remote.mockImplementation((_url: string, _token: string, query: string) => Promise.resolve({
    data: query === docsGQL ? { items: [doc], total: 1 } : { app },
  }))
  const search = scope.run(() => useGlobalSearch(ref({ ...current, urlToken: app.urlToken }), () => 'Local'))!
  search.domain.value = 'docs'
  search.submit('result')
  await vi.waitFor(() => expect(search.total.value).toBe(1))
  discovery.resolve({ data: { peers: [{ id: 'managed', name: 'Archive', managed: true, online: true }] } })
  await vi.waitFor(() => expect(search.total.value).toBe(2))
  expect(transport.current.mock.calls.filter(([query]) => query === docsGQL)).toHaveLength(1)
  expect(transport.remote.mock.calls.every(([url]) => url === 'https://current.example.test/proxy/managed/graphql')).toBe(true)
})

it('keeps failures isolated by device and searches notes and feeds with authorized file types', async () => {
  loginPeers.value = [peer]
  transport.current
    .mockResolvedValueOnce({ data: { app } })
    .mockResolvedValueOnce({ data: { items: [doc], total: 1 } })
    .mockResolvedValueOnce({ data: { items: [{ id: 'entry', title: 'Feed result' }], total: 3 } })
    .mockResolvedValueOnce({ data: { notes: [{ id: 'note', title: 'Note result' }], noteCount: 7 } })
  transport.remote.mockRejectedValue(new GqlError('unreachable'))
  const search = scope.run(() => useGlobalSearch(ref(current), () => 'Local'))!
  search.submit('result')
  await vi.waitFor(() => expect(search.groups.value.map((group) => group.status)).toEqual(['ready', 'offline']))
  await vi.waitFor(() => expect(search.total.value).toBe(11))
  expect(search.groups.value[0].pages.map((page) => [page.domain, page.hits[0].item.id]))
    .toEqual([['docs', 'doc'], ['feeds', 'entry'], ['notes', 'note']])
  expect(transport.current.mock.calls.slice(1)).toEqual([
    [docsGQL, { query: 'result trash:false', offset: 0, limit: 24, sortBy: 'DATE_DESC' }],
    [feedEntriesGQL, { query: 'result', offset: 0, limit: 24 }],
    [notesGQL, { query: 'result trash:false', offset: 0, limit: 24 }],
  ])
})

it('limits each search to its selected type until All is selected', async () => {
  transport.current.mockImplementation((document: string) => Promise.resolve({
    data: document === appGQL ? { app: { ...app, access: ['IMAGES', 'DOCS'] } } : { items: [], total: 0, images: [], imageCount: 0, notes: [], noteCount: 0 },
  }))
  const search = scope.run(() => useGlobalSearch(ref(current), () => 'Local'))!

  search.domain.value = 'images'
  search.submit('travel')
  await vi.waitFor(() => expect(transport.current.mock.calls.map(([document]) => document)).toEqual([appGQL, imagesGQL]))

  search.domain.value = 'docs'
  search.submit('beach')
  await vi.waitFor(() => expect(transport.current.mock.calls.map(([document]) => document)).toEqual([appGQL, imagesGQL, appGQL, docsGQL]))

  search.reset()
  transport.current.mockClear()
  search.domain.value = 'all'
  search.submit('travel')
  await vi.waitFor(() => expect(transport.current.mock.calls.map(([document]) => document)).toEqual([appGQL, imagesGQL, docsGQL, feedEntriesGQL, notesGQL]))
})

it('searches both devices and uses the sole remaining device when the other is removed', async () => {
  loginPeers.value = [peer]
  const remoteDoc = { ...doc, title: 'Remote result.txt', path: '/docs/remote.txt' }
  transport.current.mockImplementation((query: string) => Promise.resolve({
    data: query === appGQL ? { app } : { items: [doc], total: 1 },
  }))
  transport.remote.mockImplementation((_url: string, _token: string, query: string) => Promise.resolve({
    data: query === appGQL ? { app } : { items: [remoteDoc], total: 1 },
  }))
  const search = scope.run(() => useGlobalSearch(ref(current), () => 'Local'))!
  search.domain.value = 'docs'
  search.submit('result')
  await vi.waitFor(() => expect(search.total.value).toBe(2))
  expect(search.groups.value.map((group) => {
    const hit = group.pages[0].hits[0]
    return [group.device.id, hit.domain === 'docs' && hit.item.path]
  }))
    .toEqual([[current.clientId, doc.path], [peer.id, remoteDoc.path]])

  search.select(current.clientId, false)
  await vi.waitFor(() => expect(search.groups.value.map(({ device }) => device.id)).toEqual([peer.id]))
  loginPeers.value = []
  await vi.waitFor(() => {
    expect(search.groups.value.map(({ device }) => device.id)).toEqual([current.clientId])
    expect(search.total.value).toBe(1)
  })
})

it('uses native queries for this computer while the existing remote session remains active', async () => {
  transport.remoteId = peer.id
  loginPeers.value = [peer]
  const result = { data: { items: [doc], total: 1 } }
  transport.current.mockResolvedValueOnce({ data: { app } }).mockResolvedValueOnce(result)
  transport.local.mockResolvedValueOnce({ data: { app: { ...app, deviceName: 'Local' } } }).mockResolvedValueOnce(result)
  const search = scope.run(() => useGlobalSearch(ref({ ...current, clientId: peer.id }), () => 'Local'))!
  search.domain.value = 'docs'
  search.submit('result')
  await vi.waitFor(() => expect(search.total.value).toBe(2))
  expect(search.groups.value.map(({ device }) => device.target)).toEqual(['current', 'local'])
  expect(transport.remote).not.toHaveBeenCalled()
  const group = search.groups.value[1]
  const hit = group.pages[0].hits[0]
  if (hit.domain !== 'docs') throw new Error('Expected a document')
  const url = new URL(group.client!.url(hit))
  expect(url.origin).toBe('http://localhost:8080')
  expect(url.searchParams.get('id')).toMatch(/^g\./)
})

it('keeps an explicit device selection and prevents an old query replacing the new result', async () => {
  loginPeers.value = [peer]
  const { promise, resolve } = Promise.withResolvers<{ data: { images: []; imageCount: number } }>()
  transport.current.mockImplementation((document: string) => {
    if (document === appGQL) return Promise.resolve({ data: { app: { ...app, access: ['IMAGES', 'VIDEOS'] } } })
    if (document === imagesGQL) return promise
    if (document === videosGQL) return Promise.resolve({ data: { videos: [], videoCount: 2 } })
    throw new Error('Unexpected operation')
  })
  const search = scope.run(() => useGlobalSearch(ref(current), () => 'Local'))!
  search.select(peer.id, false)
  search.domain.value = 'images'
  search.submit('old')
  await vi.waitFor(() => expect(transport.current).toHaveBeenCalledWith(imagesGQL, expect.objectContaining({ query: 'old trash:false' })))
  search.domain.value = 'videos'
  search.submit('new')
  await vi.waitFor(() => expect(search.total.value).toBe(2))
  resolve({ data: { images: [], imageCount: 99 } })
  await nextTick()
  expect(search.total.value).toBe(2)
  expect(search.groups.value[0].pages[0].domain).toBe('videos')
  expect(transport.remote).not.toHaveBeenCalled()
  search.reset()
  await nextTick()
  expect(search.groups.value).toEqual([])
})
