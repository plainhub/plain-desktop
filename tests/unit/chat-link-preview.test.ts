import { afterEach, expect, it, vi } from 'vitest'
import { createSSRApp, h } from 'vue'
import { renderToString } from 'vue/server-renderer'
import { click, clientTemplate, content, flush, nodes, renderView } from '../helpers/vue-node-renderer'
import VIconButton from '@/components/base/VIconButton.vue'

const fixture = vi.hoisted(() => ({
  resource: vi.fn((_: unknown, image: string) => 'local-' + image),
  mutate: vi.fn(),
  openUrl: vi.fn(),
}))
vi.mock('@/stores/temp', () => ({ useTempStore: () => ({ urlTokenKey: new Uint8Array(32).fill(7) }) }))
vi.mock('@/lib/api/file', () => ({ getChatFileId: fixture.resource, getFileUrl: (id: string) => '/fs?id=' + encodeURIComponent(id) }))
vi.mock('@/lib/browser', () => ({ openUrl: fixture.openUrl }))
vi.mock('@/lib/api/mutation', async () => {
  const { ref } = await import('vue')
  return { addBookmarksGQL: 'addBookmarks', initMutation: () => ({ mutate: fixture.mutate, loading: ref(false) }) }
})
import ChatLinkPreviews from '@/views/chat/ChatLinkPreviews.vue'

const dropdown = {
  props: ['modelValue'],
  emits: ['update:modelValue'],
  setup(props: { modelValue?: boolean }, { slots, emit }: any) {
    return () => h('div', [
      h('div', { 'data-trigger': true, onClick: (event: Event) => {
        event.stopPropagation()
        emit('update:modelValue', !props.modelValue)
      } }, slots.trigger()),
      ...(props.modelValue ? slots.default() : []),
    ])
  },
}
afterEach(() => { vi.clearAllMocks() })
const props = (images: (string | null)[]) => ({ data: { content: {
  __typename: 'ChatMessageContent',
  linkPreviews: images.map((image, i) => ({
    url: `https://example.invalid/${i}`, domain: 'example.invalid', title: `Fixture ${i}`, description: null,
    image, imageWidth: i === 0 ? 200 : 30, imageHeight: i === 0 ? 200 : 30,
  })),
} } })
const view = (images: (string | null)[]) => {
  const app = createSSRApp(ChatLinkPreviews, props(images))
  app.component('VDropdown', dropdown)
  app.component('VIconButton', VIconButton)
  app.config.globalProperties.$t = (key: string) => key
  return renderToString(app)
}

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

it('saves only the selected preview as an ungrouped bookmark without opening its link', async () => {
  const component = clientTemplate({ ...ChatLinkPreviews }, 'src/views/chat/ChatLinkPreviews.vue')
  const { root, unmount } = renderView(component, props([null, null]), {
    VDropdown: dropdown,
    VIconButton: clientTemplate({ ...VIconButton }, 'src/components/base/VIconButton.vue'),
    VCircularProgress: { render: () => null },
  })
  try {
    const triggers = nodes(root).filter(element => element.props['data-trigger'])
    expect(triggers).toHaveLength(2)
    click(triggers[1]!)
    await flush()
    expect(fixture.mutate).not.toHaveBeenCalled()
    const action = nodes(root).find(element => element.tag === 'button' && content(element) === 'add_to_bookmarks')!
    const stopPropagation = vi.fn()
    action.props.onClick({ stopPropagation })
    await flush()
    expect(stopPropagation).toHaveBeenCalledOnce()
    expect(fixture.mutate).toHaveBeenCalledExactlyOnceWith({ urls: ['https://example.invalid/1'], groupId: '' })
    expect(fixture.openUrl).not.toHaveBeenCalled()
    expect(nodes(root).some(element => element.tag === 'button' && content(element) === 'add_to_bookmarks')).toBe(false)
    const cards = nodes(root).filter(element => element.props.class === 'link-preview-item')
    click(cards[1]!)
    expect(fixture.openUrl).toHaveBeenCalledExactlyOnceWith('https://example.invalid/1')
  } finally {
    unmount()
  }
})
