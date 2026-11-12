import { expect, it, vi } from 'vitest'

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
