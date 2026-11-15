import { expect, it, vi } from 'vitest'
import { h, nextTick } from 'vue'
import TextFileView from '@/views/text-file/TextFileView.vue'
import ViewerPanel from '@/views/text-file/ViewerPanel.vue'
import StatusBar from '@/views/text-file/StatusBar.vue'
import { click, clientTemplate, nodes, renderView } from '../helpers/vue-node-renderer'

const fixture = vi.hoisted(() => ({ setEditing: (_editing: boolean) => {} }))

vi.mock('@/views/text-file/text-file', async () => {
  const { ref } = await import('vue')
  return { useTextFile: () => {
    const isEditing = ref(false)
    fixture.setEditing = (editing) => { isEditing.value = editing }
    return { fileName: ref('sample.txt'), content: ref('Text to read'), isEditing, draft: ref('Text to read') }
  } }
})
vi.mock('@/components/CodeEditor.vue', () => ({ default: { render: () => h('div', 'text editor') } }))
vi.mock('@/components/jsonviewer/json-viewer.vue', () => ({ default: { render: () => null } }))
vi.mock('@/views/text-file/ViewerToolbar.vue', () => ({ default: { render: () => h('div', { class: 'viewer-toolbar' }) } }))

it('keeps only the viewer area and its exit control in fullscreen, then restores the page', async () => {
  vi.stubGlobal('__IS_TAURI__', true)
  clientTemplate(ViewerPanel, 'src/views/text-file/ViewerPanel.vue')
  clientTemplate(StatusBar, 'src/views/text-file/StatusBar.vue')
  const component = clientTemplate(TextFileView, 'src/views/text-file/TextFileView.vue')
  const view = renderView(component, {}, {
    HeaderActions: { render: () => null },
    VCircularProgress: { render: () => null },
    VCopyButton: { render: () => h('button', 'copy') },
    VOutlinedButton: { render: () => h('button') },
  })
  const byClass = (name: string) => nodes(view.root).find(node => node.props.class?.split(' ').includes(name))
  try {
    const area = byClass('viewer-area')!
    expect(byClass('topbar')).toBeDefined()
    expect(byClass('status-bar')).toBeDefined()
    click(nodes(area).find(node => node.props['aria-label'] === 'fullscreen')!)
    await nextTick()

    expect(byClass('topbar')).toBeUndefined()
    expect(byClass('status-bar')).toBeUndefined()
    expect(byClass('viewer-area')).toBe(area)
    click(nodes(area).find(node => node.props['aria-label'] === 'exit_fullscreen')!)
    await nextTick()

    expect(byClass('topbar')).toBeDefined()
    expect(byClass('status-bar')).toBeDefined()
    expect(byClass('viewer-area')).toBe(area)

    click(nodes(area).find(node => node.props['aria-label'] === 'fullscreen')!)
    await nextTick()
    fixture.setEditing(true)
    await nextTick()
    expect(byClass('topbar')).toBeDefined()
    expect(byClass('editor')).toBeDefined()
    expect(byClass('viewer-area')).toBeUndefined()
  } finally {
    view.unmount()
    vi.unstubAllGlobals()
  }
})
