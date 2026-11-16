import { afterEach, expect, it, vi } from 'vitest'
import { createSSRApp } from 'vue'
import { renderToString } from 'vue/server-renderer'

const fixture = vi.hoisted(() => ({ resource: vi.fn((_: unknown, image: string) => 'local-' + image) }))
vi.mock('@/stores/temp', () => ({ useTempStore: () => ({ urlTokenKey: new Uint8Array(32).fill(7) }) }))
vi.mock('@/lib/api/file', () => ({ getChatFileId: fixture.resource, getFileUrl: (id: string) => '/fs?id=' + encodeURIComponent(id) }))
vi.mock('@/lib/browser', () => ({ openUrl: vi.fn() }))
import ChatLinkPreviews from '@/views/chat/ChatLinkPreviews.vue'

afterEach(() => { fixture.resource.mockClear() })
const view = (images: (string | null)[]) => renderToString(createSSRApp(ChatLinkPreviews, { data: { content: {
  __typename: 'ChatMessageContent',
  linkPreviews: images.map((image, i) => ({
    url: `https://example.invalid/${i}`, domain: 'example.invalid', title: `Fixture ${i}`, description: null,
    image, imageWidth: i === 0 ? 200 : 30, imageHeight: i === 0 ? 200 : 30,
  })),
} } }))

it('shows an authorized local reference in large and small cards', async () => {
  const html = await view(['fid:' + 'a'.repeat(64), 'fid:' + 'b'.repeat(64)])
  expect(html).toContain('class="preview-image"')
  expect(html).toContain('class="preview-small-icon"')
  expect(html).toContain('/fs?id=local-fid%3A' + 'a'.repeat(64))
  expect(html).toContain('/fs?id=local-fid%3A' + 'b'.repeat(64))
  expect(fixture.resource).toHaveBeenCalledTimes(2)
})

it('does not load nullable, external or filesystem paths', async () => {
  const html = await view([null, 'https://tracker.invalid/pixel.svg', '/private/non-public.png'])
  expect(html).not.toContain('<img')
  expect(html).not.toContain('tracker.invalid')
  expect(html).not.toContain('/private/')
  expect(fixture.resource).not.toHaveBeenCalled()
  expect(html).toContain('Fixture 0')
})
