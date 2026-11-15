import { expect, it, vi } from 'vitest'
import { createSSRApp } from 'vue'
import { renderToString } from 'vue/server-renderer'
import EventCard from '@/views/settings/events/EventCard.vue'
import type { EventRecord } from '@/views/settings/events/api'
import messages from '@/locales/en-US/events'

vi.mock('@/lib/format', () => ({ formatDateTimeFull: (value: string) => `Full ${value}`, formatTime: () => '23:24' }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => messages[key as keyof typeof messages] ?? key }) }))

const entry: EventRecord = {
  id: 'fixture-event', type: 'LOGIN', data: { __typename: 'LoginEventData', sessionType: 'LOGIN' }, clientId: '706c6169-b9ef-8b64-b37d-1162ef232835',
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
  for (const text of ['fixture-event', '706c6169-b9ef-8b64-b37d-1162ef232835', 'View raw', 'View details', '<details', '<button', 'event-icon']) expect(html).not.toContain(text)
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
  const html = await render({ ...entry, type: 'LOGIN_FAILED', clientName: '', data: { __typename: 'LoginFailedEventData', reason: 'INVALID_PASSWORD' }, osName: '', osVersion: '', appName: '', appVersion: '' })
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
  const html = await render({ ...system, type: 'MOUNT', data: { __typename: 'VolumeEventData', volume: { mountPath: '/media/fixture' } }, clientName: '', clientIp: '', osName: '', osVersion: '', appName: '', appVersion: '' })
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

 it('shows the revoked session separately from the actor snapshot', async () => {
  const html = await render({ ...entry, type: 'REVOKE_SESSION', data: {
    __typename: 'SessionEventData', session: { clientId: '706c6169-b0c0-890c-9e60-9b0ce9b00aab', clientName: 'Fixture target', type: 'API_TOKEN' },
  } })
  expect(html).toContain('Study computer')
  expect(html).toContain('Fixture target')
})

const volume = { id: 'fsuuid:fixture', name: 'Fixture volume', devicePath: '/dev/fixture1', mountPath: '/mnt/fixture', fileSystem: 'ext4' }
const disk = { id: 'disk-fixture', name: 'Fixture disk', path: '/dev/fixture', sizeBytes: 64000000000 }

it.each([
  ['LOGIN', { __typename: 'LoginEventData', sessionType: 'LOGIN' }, ['Signed in']],
  ['LOGIN_FAILED', { __typename: 'LoginFailedEventData', reason: 'REJECTED' }, ['Sign-in failed', 'Sign-in request rejected']],
  ['LOGOUT', { __typename: 'SessionEventData', session: { clientId: '706c6169-1ac2-825a-8c79-0951ec181d4a', clientName: 'Exited client', type: 'LOGIN' } }, ['Signed out', 'Exited client']],
  ['REVOKE_SESSION', { __typename: 'SessionEventData', session: { clientId: '706c6169-15b2-8ac4-83ea-4e6b0c3a4b3c', clientName: 'Target token', type: 'API_TOKEN' } }, ['Session revoked', 'Target token']],
  ['UPDATE_DEVICE_NAME', { __typename: 'DeviceNameEventData', oldName: 'Old device', newName: 'New device' }, ['Device name changed', 'Old device → New device']],
  ['MOUNT', { __typename: 'VolumeEventData', volume }, ['Disk mounted', 'Fixture volume', '/mnt/fixture']],
  ['MOUNT_FAILED', { __typename: 'VolumeFailedEventData', volume, failure: { reason: 'MOUNT_FAILED', detail: 'Could not mount filesystem' } }, ['Disk mount failed', 'Could not mount filesystem']],
  ['UNMOUNT', { __typename: 'VolumeEventData', volume }, ['Disk unmounted', 'Fixture volume', '/mnt/fixture']],
  ['FORMAT_DISK', { __typename: 'DiskFormatEventData', disk, fileSystem: 'ext4' }, ['Disk formatted', 'Fixture disk', '/dev/fixture', 'ext4']],
  ['FORMAT_DISK_FAILED', { __typename: 'DiskFormatFailedEventData', disk, fileSystem: 'ext4', failure: { reason: 'FORMAT_DISK_FAILED' } }, ['Disk format failed', 'Fixture disk', '/dev/fixture', 'Format failed']],
] satisfies [EventRecord['type'], EventRecord['data'], string[]][])('renders %s from the saved snapshot', async (type, data, expected) => {
  const html = await render({ ...entry, type, data })
  for (const text of [...expected, 'Study computer']) expect(html).toContain(text)
  expect(html.includes('event-error')).toBe(type.endsWith('_FAILED'))
})

it('escapes object names and failure details as text', async () => {
  const html = await render({ ...entry, type: 'MOUNT_FAILED', data: {
    __typename: 'VolumeFailedEventData', volume: { ...volume, name: '<script>unsafe</script>' },
    failure: { reason: 'MOUNT_FAILED', detail: '<img src=x onerror=unsafe>' },
  } })
  expect(html).toContain('&lt;script&gt;')
  expect(html).not.toContain('<script>')
  expect(html).not.toContain('<img src=x')
})
