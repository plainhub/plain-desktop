import { afterEach, expect, it, vi } from 'vitest'
import { createSSRApp } from 'vue'
import { renderToString } from 'vue/server-renderer'
import { xchacha20poly1305 } from '@noble/ciphers/chacha'
vi.mock('@/stores/temp', () => ({ useTempStore: () => ({ urlTokenKey: new Uint8Array(32).fill(7) }) }))
vi.mock('@/hooks/open-media', () => ({ useOpenMedia: () => ({ open: vi.fn() }) }))
vi.mock('@/views/chat/ChatDownloadOverlay.vue', () => ({ default: { render: () => null } }))
vi.mock('@/lib/file', () => ({ isVideo: () => false }))
vi.mock('@/lib/format', () => ({ formatDurationMs: () => '', formatFileSize: () => '' }))
vi.mock('@/lib/api/api', () => ({ getApiHeaders: () => ({ 'c-id': 'test-client' }), getProxyUrl: (path: string) => 'https://fixture.invalid' + path }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => false }))
vi.mock('@/components/toaster', () => ({ default: vi.fn(), toastWithAction: vi.fn() }))
vi.mock('@/lib/download-dir', () => ({ downloadToDir: vi.fn(), downloadAs: vi.fn(), revealInFolder: vi.fn() }))
vi.mock('@/hooks/native-resource-url', () => ({ nativeResourceUrl: vi.fn() }))
import ChatImages from '@/views/chat/ChatImages.vue'
afterEach(() => vi.unstubAllGlobals())
it('encodes the stored chat image as an app-file resource rather than a legacy JSON filename', async () => {
  vi.stubGlobal('window', { fileIdMap: new Map() })
  const hash = 'a'.repeat(64)
  const html = await renderToString(createSSRApp(ChatImages, { data: { _content: { value: { items: [{ uri: 'fid:' + hash, fileName: 'image.png', size: 10 }] } } } }))
  expect(html).toContain('class="image-thumb"')
  const id = new URL(html.match(/src="([^"]+)"/)![1]!).searchParams.get('id')!
  const payload = Buffer.from(id.slice(id.lastIndexOf('.') + 1), 'base64url')
  const plain = xchacha20poly1305(new Uint8Array(32).fill(7), payload.subarray(0, 24), new TextEncoder().encode('plain-fs-v1\0c\0test-client')).decrypt(payload.subarray(24))
  expect(JSON.parse(new TextDecoder().decode(plain))).toEqual({ type: 'APP_FILE', id: hash })
})

it('loads a remote thumbnail using only the encrypted peer file reference', async () => {
  const html = await renderToString(createSSRApp(ChatImages, {
    peer: { id: 'peer-1' },
    data: { _content: { value: { items: [
      { uri: 'fsid:peer-resource-id', fileName: 'image.png', size: 10 },
    ] } } },
  }))
  const url = new URL(html.match(/src="([^"]+)"/)![1]!.replaceAll('&amp;', '&'))
  expect(url.pathname).toBe('/proxy/peer-1/fs')
  expect(url.searchParams.get('size')).toBe('sm')
  expect(url.searchParams.get('action')).toBe('preview')
  expect(url.href).not.toContain('peer-resource-id')
  expect([...url.searchParams.keys()].sort()).toEqual(['action', 'id', 'size'])
  const token = url.searchParams.get('id')!
  const payload = Buffer.from(token.slice(token.lastIndexOf('.') + 1), 'base64url')
  const plain = xchacha20poly1305(new Uint8Array(32).fill(7), payload.subarray(0, 24),
    new TextEncoder().encode('plain-chat-attachment:peer-1-v1\0c\0test-client')).decrypt(payload.subarray(24))
  expect(new TextDecoder().decode(plain)).toBe('fsid:peer-resource-id')
})
