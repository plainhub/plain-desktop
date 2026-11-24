import { h, ref, type SetupContext } from 'vue'
import { expect, it, vi } from 'vitest'
import MediaSidebar from '@/components/MediaSidebar.vue'
import SidebarListItem from '@/components/SidebarListItem.vue'
import { DataType } from '@/lib/data'
import { click, clientTemplate, content, flush, nodes, renderView } from '../helpers/vue-node-renderer'

vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => local.value }))
vi.mock('@/hooks/media-sidebar', () => ({ useMediaSidebar: () => ({
  total: 42, totalTrash, trash: true, viewTrash,
}) }))

const local = ref(false)
const totalTrash = ref(18)
const viewTrash = vi.fn()

clientTemplate(MediaSidebar, 'src/components/MediaSidebar.vue')
clientTemplate(SidebarListItem, 'src/components/SidebarListItem.vue')

const components = {
  LeftSidebar: { setup: (_: unknown, { slots }: SetupContext) => () => h('aside', slots.body?.()) },
  SidebarListItem,
  BucketFilter: () => null,
  ExtFilter: () => null,
  TagFilter: () => null,
}

it.each([DataType.IMAGE, DataType.VIDEO, DataType.AUDIO, DataType.DOC])('renders the shared %s Trash entry on Desktop and Web', async (type) => {
  const view = renderView(MediaSidebar, { type, gql: 'fixture' }, components)
  const row = () => nodes(view.root).find(node => node.tag === 'li' && content(node).startsWith('trash'))
  try {
    local.value = true
    await flush()
    expect(row()).toBeDefined()
    local.value = false
    await flush()
    expect(row()).toBeDefined()
  } finally {
    local.value = false
    view.unmount()
  }
})

it('anchors remote count help to the whole Trash row and leaves local counts plain', async () => {
  const view = renderView(MediaSidebar, { type: DataType.IMAGE, gql: 'fixture' }, components)
  const row = () => nodes(view.root).find(node => node.tag === 'li' && content(node).startsWith('trash'))!
  const end = () => row().children.find(node => node.props.class === 'end')
  try {
    expect(row().props.class.split(' ')).toContain('media-trash-row')
    const count = end()!.children.filter(node => !node.tag.startsWith('#'))
    expect(count).toHaveLength(1)
    expect(count[0]!.tag).toBe('button')
    expect(content(count[0]!).trim()).toBe('18')
    expect(nodes(count[0]!).some(node => node.tag === 'i-material-symbols:help-outline-rounded')).toBe(true)
    click(row())
    expect(viewTrash).toHaveBeenCalledOnce()

    totalTrash.value = -1
    await flush()
    expect(end()).toBeUndefined()

    local.value = true
    totalTrash.value = 0
    await flush()
    expect(content(end()!)).toBe('0')
    expect(nodes(end()!).some(node => node.tag === 'button')).toBe(false)
  } finally { view.unmount() }
})
