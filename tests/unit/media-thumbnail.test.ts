import { afterEach, expect, it, vi } from 'vitest'
import { createSSRApp, h } from 'vue'
import { renderToString } from 'vue/server-renderer'
import { xchacha20poly1305 } from '@noble/ciphers/chacha'
import ImageListItem from '@/views/images/ImageListItem.vue'
import VideoListItem from '@/views/videos/VideoListItem.vue'
import { getFileId, getFileUrl } from '@/lib/api/file'

const state = vi.hoisted(() => ({ local: true }))
const key = new Uint8Array(32).fill(7)
const owner = '706c6169-e2eb-85ac-b78e-a2754bf184e8'
vi.mock('@/lib/api/api', () => ({
  getApiHeaders: () => ({ 'c-id': '706c6169-e2eb-85ac-b78e-a2754bf184e8' }),
  getProxyUrl: (path: string) => 'http://localhost:8080' + path,
  getUrlToken: () => btoa(String.fromCharCode(...new Uint8Array(32).fill(7))),
}))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => __IS_TAURI__ && state.local }))
vi.mock('@/components/toaster', () => ({ default: vi.fn(), toastWithAction: vi.fn() }))
vi.mock('@/lib/download-dir', () => ({ downloadToDir: vi.fn(), downloadAs: vi.fn(), revealInFolder: vi.fn() }))

afterEach(() => vi.unstubAllGlobals())

function resource(url: URL) {
  const id = url.searchParams.get('id')!
  const payload = Buffer.from(id.slice(id.lastIndexOf('.') + 1), 'base64url')
  return JSON.parse(new TextDecoder().decode(xchacha20poly1305(key, payload.subarray(0, 24),
    new TextEncoder().encode(['FS', 'c', owner].join('.'))).decrypt(payload.subarray(24))))
}

it.each([
  { native: true, local: true, type: 'MEDIA' },
  { native: true, local: false, type: 'MEDIA' },
  { native: false, local: true, type: 'MEDIA' },
])('renders scoped thumbnails while preserving normal file references ($native, $local)', async ({ native, local, type }) => {
  vi.stubGlobal('__IS_TAURI__', native)
  vi.stubGlobal('window', { fileIdMap: new Map() })
  vi.stubGlobal('localStorage', { getItem: () => null })
  state.local = local
  for (const [component, extension] of [[ImageListItem, 'svg'], [ImageListItem, 'png'], [VideoListItem, 'mp4']] as const) {
    const item = { id: 'indexed-image', path: `/fixtures/image.${extension}`, size: 355, updatedAt: '2026-01-01T00:00:00Z', tags: [], bucketId: 'bucket', fileId: '' }
    item.fileId = getFileId(key, item.path, item.id)
    for (const isPhone of [false, true]) {
      const app = createSSRApp(component, {
        item, index: 0, selectedIds: [], shiftEffectingIds: [], shouldSelect: false, isPhone, imageErrorIds: [],
        bucketsMap: {}, filter: {}, dataType: 'IMAGE', tags: [], mainStore: {}, app: {},
        restoreLoading: () => false, trashLoading: () => false,
      })
      for (const name of ['VCheckbox', 'FieldId', 'ItemTags', 'ImageActionButtons', 'VideoActionButtons']) app.component(name, () => null)
      app.component('ListItemPhone', (_, { slots }) => h('div', slots.image?.()))
      app.directive('tooltip', {})
      const html = await renderToString(app)
      const url = new URL(html.match(/<img[^>]+data-src="([^"]+)"/)![1]!.replaceAll('&amp;', '&'))
      expect(url.pathname).toBe('/fs')
      expect(url.searchParams.get('size')).toBe('SM')
      expect(url.searchParams.get('v')).toBe(item.updatedAt)
      expect(url.searchParams.get('action')).toBe('PREVIEW')
      expect(resource(url)).toEqual({ type, id: item.id })
      expect(resource(new URL(getFileUrl(item.fileId)))).toEqual({ type: 'MEDIA', id: item.id })
    }
  }
})
