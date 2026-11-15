import { expect, it, vi } from 'vitest'
import { createSSRApp } from 'vue'
import { renderToString } from 'vue/server-renderer'
import SessionCard from '@/views/settings/sessions/SessionCard.vue'
import type { SessionRecord } from '@/views/settings/sessions/api'
import VTextButton from '@/components/base/VTextButton.vue'
import VIconButton from '@/components/base/VIconButton.vue'
import InlineDeleteConfirm from '@/components/base/InlineDeleteConfirm.vue'
import { CopyButton } from '../helpers/vue-node-renderer'

vi.mock('@/lib/format', () => ({ formatDateTime: (value: string) => value }))

const entry: SessionRecord = {
  clientId: 'fixture-client', clientName: 'Fixture browser', type: 'LOGIN',
  clientIp: '192.0.2.24', osName: 'Fixture OS', osVersion: '1', appName: 'Fixture app', appVersion: '2',
  createdAt: '2026-01-01T00:00:00Z', lastSeenAt: '2026-01-02T00:00:00Z',
}
async function render(record: SessionRecord, online?: boolean) {
  const app = createSSRApp(SessionCard, { entry: record, online, busy: false })
  app.config.globalProperties.$t = (key: string) => key
  app.component('VCopyButton', CopyButton)
  app.component('VTextButton', VTextButton)
  app.component('VIconButton', VIconButton)
  app.component('InlineDeleteConfirm', InlineDeleteConfirm)
  return renderToString(app)
}

it('shows saved client facts, activity and a known offline state', async () => {
  const html = await render(entry, false)
  for (const text of ['Fixture browser', 'Fixture OS 1', 'Fixture app 2', '192.0.2.24', entry.createdAt, entry.lastSeenAt!]) expect(html).toContain(text)
  expect(html).toContain('session_management.offline')
  expect(html).toContain('session_management.revoke_session')
})

it('distinguishes unknown online status and an online client', async () => {
  const unknown = await render(entry)
  expect(unknown).not.toContain('session_management.offline')
  expect(unknown).not.toContain('session_management.online')
  const online = await render(entry, true)
  expect(online).toContain('session_management.online')
  expect(online).not.toContain('session_management.last_active')
})

it.each(['LOGIN', 'API_TOKEN'] as const)('shows the client ID for %s sessions', async type => {
  const html = await render({ ...entry, type })
  expect(html).toContain(entry.clientId)
  expect(html).toContain('client_id')
  expect(html).toContain('session_management.view_raw')
})
