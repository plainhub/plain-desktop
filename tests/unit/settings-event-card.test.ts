import { expect, it, vi } from 'vitest'
import { createSSRApp } from 'vue'
import { renderToString } from 'vue/server-renderer'
import EventCard from '@/views/settings/events/EventCard.vue'
import type { EventRecord } from '@/views/settings/events/api'
import messages from '@/locales/en-US/events'

vi.mock('@/lib/format', () => ({ formatDateTimeFull: (value: string) => `Full ${value}`, formatTime: () => '23:24' }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ te: (key: string) => key in messages }) }))

const entry: EventRecord = {
  id: 'fixture-event', type: 'LOGIN', message: '', clientId: 'fixture-client',
  clientName: 'Study computer', clientIp: '192.0.2.24', osName: 'Windows', osVersion: '11',
  appName: 'Chrome', appVersion: '142', createdAt: '2026-11-14T15:24:00Z',
}

async function render(record: EventRecord) {
  const app = createSSRApp(EventCard, { entry: record })
  app.directive('tooltip', {})
  app.config.globalProperties.$t = (key: string) => messages[key as keyof typeof messages] ?? key
  return renderToString(app)
}

it('shows saved event facts without visible raw or details controls', async () => {
  const html = await render(entry)
  for (const text of ['Signed in', 'Study computer', 'Chrome 142', 'Windows 11', '192.0.2.24', '23:24']) expect(html).toContain(text)
  for (const text of ['fixture-event', 'fixture-client', 'View raw', 'View details', '<details', '<button', 'event-icon']) expect(html).not.toContain(text)
  expect(html).toContain('aria-haspopup="dialog"')
  expect(html).toContain('aria-expanded="false"')
})

it('omits missing browser names instead of inventing a client label', async () => {
  const html = await render({ ...entry, clientName: '' })
  expect(html).not.toContain('event-client')
  expect(html).not.toContain('Unnamed client')
  expect(html).toContain('Chrome 142')
})

it('shows a translated failure reason and the actual available connection information', async () => {
  const html = await render({ ...entry, type: 'LOGIN_FAILED', clientName: '', message: 'invalid_password', osName: '', osVersion: '', appName: '', appVersion: '' })
  expect(html).toContain('Sign-in failed')
  expect(html).not.toContain('Unrecognized client')
  expect(html).not.toContain('event-client')
  expect(html).toContain('Incorrect password')
  expect(html).toContain('192.0.2.24')
  expect(html).not.toContain('Windows')
  expect(html).not.toContain('Chrome')
})

it('keeps system events useful without inventing client details', async () => {
  const { clientId: _, ...system } = entry
  const html = await render({ ...system, type: 'MOUNT', message: '/media/fixture', clientName: '', clientIp: '', osName: '', osVersion: '', appName: '', appVersion: '' })
  expect(html).toContain('Disk mounted')
  expect(html).toContain('/media/fixture')
  expect(html).not.toContain('Unrecognized client')
  expect(html).not.toContain('event-facts')
})


it('shows available versions when client software names are missing', async () => {
  const html = await render({ ...entry, appName: '', osName: '' })
  expect(html).toContain('142')
  expect(html).toContain('11')
  expect(html).not.toContain('Chrome')
  expect(html).not.toContain('Windows')
})
