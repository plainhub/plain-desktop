import { xchacha20poly1305 } from '@noble/ciphers/chacha'
import { afterEach, expect, it, vi } from 'vitest'
import { createSSRApp, h, type Slots } from 'vue'
import { renderToString } from 'vue/server-renderer'
import ContactListItem from '@/views/contacts/ContactListItem.vue'
import { DataType } from '@/lib/data'

const session = vi.hoisted(() => ({
  clientId: '706c6169-e2eb-85ac-b78e-a2754bf184e8',
  urlTokenKey: new Uint8Array(32).fill(7) as Uint8Array | null,
}))
vi.mock('@/stores/temp', () => ({ useTempStore: () => session }))
vi.mock('@/lib/api/api', () => ({
  getApiHeaders: () => ({ 'c-id': session.clientId }),
  getProxyUrl: (path: string) => 'https://fixture.invalid' + path,
}))
vi.mock('@/lib/format', () => ({ formatDateTime: () => '', formatTimeAgo: () => '' }))
vi.mock('@/views/contacts/ContactActionButtons.vue', () => ({ default: () => null }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => false }))
vi.mock('@/components/toaster', () => ({ default: vi.fn(), toastWithAction: vi.fn() }))
vi.mock('@/lib/download-dir', () => ({ downloadToDir: vi.fn(), downloadAs: vi.fn(), revealInFolder: vi.fn() }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))

afterEach(() => {
  vi.unstubAllGlobals()
  session.urlTokenKey = new Uint8Array(32).fill(7)
})

const thumbnail = 'content://com.android.contacts/contacts/42/photo'

async function render(isPhone: boolean, thumbnailId = thumbnail) {
  const app = createSSRApp(ContactListItem, {
    item: { id: '42', firstName: 'Test', thumbnailId, phoneNumbers: [], emails: [], addresses: [], websites: [], ims: [], events: [], tags: [], updatedAt: '2026-01-01T00:00:00Z' },
    index: 0, selectedIds: [], shiftEffectingIds: [], shouldSelect: false, isPhone, dataType: DataType.CONTACT, tags: [],
    handleItemClick: () => {}, handleMouseOver: () => {}, toggleSelect: () => {},
  })
  app.config.globalProperties.$t = (key: string) => key
  app.directive('tooltip', {})
  app.component('VIconButton', () => null)
  app.component('VCheckbox', () => null)
  app.component('ListItemPhone', (_: unknown, { slots }: { slots: Slots }) => h('article', slots.image?.()))
  app.component('FieldId', () => null)
  app.component('ItemTags', () => null)
  return renderToString(app)
}

function avatarResource(html: string) {
  const src = /<img[^>]+src="([^"]+)"/.exec(html)?.[1]
  expect(src).toBeTruthy()
  const url = new URL(src!.replaceAll('&amp;', '&'))
  expect(url.pathname).toBe('/fs')
  expect(url.searchParams.get('action')).toBe('PREVIEW')
  const id = url.searchParams.get('id')!
  expect(id).not.toContain('content://')
  const payload = Buffer.from(id.slice(id.lastIndexOf('.') + 1), 'base64url')
  const aad = new TextEncoder().encode(`FS.c.${session.clientId}`)
  return JSON.parse(new TextDecoder().decode(xchacha20poly1305(session.urlTokenKey!, payload.subarray(0, 24), aad).decrypt(payload.subarray(24))))
}

it.each([false, true])('encrypts the Android contact avatar URI for the file endpoint (compact: %s)', async (isPhone) => {
  vi.stubGlobal('window', {})
  expect(avatarResource(await render(isPhone))).toEqual({ type: 'FILE', id: thumbnail })
})

it('keeps the default icon when the contact has no photo', async () => {
  expect(await render(false, '')).not.toContain('<img')
})

it('does not request a raw or empty avatar URL while the session key is unavailable', async () => {
  session.urlTokenKey = null
  expect(await render(false)).not.toContain('<img')
})
