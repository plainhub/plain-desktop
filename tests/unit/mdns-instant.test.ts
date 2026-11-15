import { afterEach, expect, it, vi } from 'vitest'

const request = vi.hoisted(() => vi.fn())
vi.mock('@/lib/api/gql-client', () => ({ gqlFetch: request }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => false }))
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
vi.mock('@/components/toaster', () => ({ default: vi.fn() }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))

import { useMdns, type MdnsActivity } from '@/views/home/remote-access/use-mdns'

it('preserves RFC3339 activity instants for local-time rendering', async () => {
  const entry: MdnsActivity = { time: '2026-11-12T08:09:10.123Z', event: 'query', detail: 'fixture' }
  request.mockResolvedValue({ data: { mdnsState: { services: [], activity: [entry] } } })
  const mdns = useMdns()
  await mdns.refreshSnapshot()
  expect(mdns.activity.value).toEqual([entry])
  expect(new Date(mdns.activity.value[0].time).toISOString()).toBe(entry.time)
})

afterEach(() => { vi.useRealTimers(); vi.restoreAllMocks() })

it('acquires and releases the Web debug session through the main schema', async () => {
  vi.useFakeTimers()
  request.mockClear()
  request.mockImplementation(async (query: string) => ({ data: query.includes('mdnsState') ? { mdnsState: { services: [], activity: [] } } : { result: true } }))
  const mdns = useMdns()
  await mdns.startBrowsing()
  const [document, variables] = request.mock.calls[0]
  expect(document).toContain('acquireMdnsDebugSession')
  expect(variables.token).toMatch(/^[0-9a-f-]{36}$/)
  expect(request.mock.calls[1][0]).toContain('mdnsState')
  mdns.stopBrowsing()
  await Promise.resolve()
  expect(request.mock.calls[2][0]).toContain('releaseMdnsDebugSession')
  expect(request.mock.calls[2][1]).toEqual(variables)
  expect(vi.getTimerCount()).toBe(0)
})
