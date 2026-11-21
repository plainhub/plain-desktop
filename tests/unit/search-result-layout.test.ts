import { readFileSync } from 'node:fs'
import { compileStyle, parse } from '@vue/compiler-sfc'
import { defineComponent, h, nextTick, provide, ref } from 'vue'
import { expect, it, vi } from 'vitest'
import DocListItem from '@/views/docs/DocListItem.vue'
import Sidebar2ListItem from '@/components/Sidebar2ListItem.vue'
import GlobalSearchView from '@/views/search/GlobalSearchView.vue'
import { globalSearchKey, type GlobalSearch } from '@/hooks/global-search'
import { clientTemplate, nodes, renderView } from '../helpers/vue-node-renderer'

vi.mock('vue-router', () => ({ useRouter: () => ({ resolve: ({ path }: { path: string }) => ({ href: path }) }) }))
vi.mock('@/hooks/open-media', () => ({ useOpenMedia: () => ({ open: vi.fn() }) }))
vi.mock('@/stores/main', () => ({ useMainStore: () => ({}) }))
vi.mock('@/views/docs/DocActionButtons.vue', () => ({ default: defineComponent({ setup: () => () => h('div', { 'data-doc-actions': true }) }) }))
vi.mock('@/components/OnlinePreviewIcon.vue', () => ({ default: defineComponent({ setup: () => () => h('span') }) }))
vi.mock('@/components/media/MediaGridItem.vue', () => ({ default: defineComponent({ setup: () => () => h('div') }) }))

vi.mock('@/components/base/VListItem.vue', () => ({ default: defineComponent({ setup: () => () => h('li') }) }))

const filename = 'src/views/search/GlobalSearchView.vue'
clientTemplate(GlobalSearchView, filename)
clientTemplate(DocListItem, 'src/views/docs/DocListItem.vue')
clientTemplate(Sidebar2ListItem, 'src/components/Sidebar2ListItem.vue')

it('spaces visible search groups consistently and reuses the document and sidebar result components', async () => {
  vi.stubGlobal('__IS_TAURI__', false)
  vi.stubGlobal('localStorage', { getItem: () => null })
  const device = { id: 'device', name: 'Test device', target: 'current' }
  const hits = (['images', 'videos', 'docs', 'feeds', 'notes'] as const).map(domain => ({
    domain, item: { id: domain, title: domain, path: '/' + domain, size: 0, updatedAt: '2026-11-21T00:00:00Z', durationMs: 0, feedId: 'feed', author: 'Author', publishedAt: '2026-11-21T00:00:00Z' },
  }))
  const otherDevice = { ...device, id: 'other' }
  const pages = ref(hits.map(hit => ({ domain: hit.domain, hits: [hit], total: hit.domain === 'videos' ? 2 : 1 })))
  const search = {
    groups: ref([{ device, status: 'ready', pages }, { device: otherDevice, status: 'offline', pages: [] }]),
    targets: ref([device, otherDevice]), total: ref(hits.length), request: ref({}),
  } as unknown as GlobalSearch
  const view = renderView(defineComponent({
    setup() {
      provide(globalSearchKey, { search, sidebarCollapsed: ref(false) })
      return () => h(GlobalSearchView)
    },
  }), {}, { VCircularProgress: () => null, VTextButton: () => h('button'), VCheckbox: () => h('input'), FieldId: () => null, ItemTags: () => null })
  try {
    const all = nodes(view.root)
    const deviceGroups = all.find(node => node.props.class === 'scroll-content')!.children.filter(node => node.tag === 'section')
    const [ready, offline] = deviceGroups
    const childGroups = (parent: typeof view.root) => parent.children.filter(node => node.props.class === 'search-group')
    const groupedDevices = childGroups(ready!.parent!)
    expect(groupedDevices).toHaveLength(2)
    expect(groupedDevices[0]).toBe(ready)
    expect(groupedDevices[1]).toBe(offline)
    const resultGroups = childGroups(ready!)
    expect(resultGroups).toHaveLength(5)
    expect(resultGroups.slice(0, 3).map(group => group.children.find(node => node.tag === 'div')?.props.class)).toEqual(['media-grid', 'media-grid', 'main-list'])
    expect(nodes(resultGroups[1]!).some(node => node.props.class === 'secondary-actions')).toBe(true)
    expect(resultGroups.slice(3).every(group => nodes(group).some(node => String(node.props.class).includes('sidebar2-item')))).toBe(true)
    const { descriptor } = parse(readFileSync(new URL(`../../${filename}`, import.meta.url), 'utf8'))
    const spacing = descriptor.styles[0]?.content ?? ''
    const { code } = compileStyle({ filename, id: 'data-v-search', source: spacing, scoped: true })
    expect(code).toMatch(/\.search-group\s*\+\s*\.search-group\[data-v-search\]\s*\{\s*margin-block-start:\s*16px/)
    expect(all.filter(node => String(node.props.class).includes('doc-item') && String(node.props.class).includes('read-only'))).toHaveLength(1)
    expect(all.filter(node => String(node.props.class).includes('sidebar2-item'))).toHaveLength(2)
    expect(all.filter(node => node.tag === 'a').map(node => node.props.href)).toEqual(['/feeds/feed/entries/feeds', '/notes/notes'])
    expect(all.some(node => node.tag === 'input' || node.props['data-doc-actions'])).toBe(false)

    pages.value[2] = { domain: 'docs', hits: [], total: 0 }
    await nextTick()
    expect(childGroups(ready!)).toHaveLength(4)
  } finally {
    view.unmount()
    vi.unstubAllGlobals()
  }
})
