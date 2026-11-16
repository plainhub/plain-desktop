import { expect, it, vi } from 'vitest'
import { h, nextTick } from 'vue'
import MarkdownEditor from '@/components/MarkdownEditor.vue'
import NoteEditView from '@/views/notes/NoteEditView.vue'
import NoteModeToggle from '@/views/notes/NoteModeToggle.vue'
import { click, clientTemplate, nodes, renderView } from '../helpers/vue-node-renderer'

vi.mock('@/hooks/note-edit', async () => {
  const { ref } = await import('vue')
  return { useNoteEdit: () => {
    const viewMode = ref('editor')
    return { content: ref('# Synthetic note'), markdown: ref('<h1>Synthetic note</h1>'), viewMode, setViewMode: (mode: string) => { viewMode.value = mode } }
  } }
})
vi.mock('@/components/MarkdownEditor.vue', () => ({ default: {
  props: ['modelValue', 'source'],
  setup: (props: { modelValue: string; source: boolean }) => () => h('div', { class: 'test-editor', 'data-source': props.source }, props.modelValue),
} }))

it('routes source mode to the same editor and displays only the selected pane', async () => {
  clientTemplate(NoteModeToggle, 'src/views/notes/NoteModeToggle.vue')
  const component = clientTemplate(NoteEditView, 'src/views/notes/NoteEditView.vue')
  const view = renderView(component, {}, { MarkdownEditor, VIconButton: { render: () => h('button') }, ItemTags: { render: () => null }, FieldId: { render: () => null }, TagRelationsDropdown: { render: () => null }, VCircularProgress: { render: () => null } })
  const byClass = (name: string) => nodes(view.root).find(node => node.props.class?.split(' ').includes(name))!
  const select = async (label: string) => {
    click(nodes(view.root).find(node => node.props['aria-label'] === label)!)
    await nextTick()
  }
  try {
    const editor = byClass('test-editor')
    expect(editor.props['data-source']).toBe(false)
    expect(byClass('preview-pane').style.display).toBe('none')
    await select('markdown_source')
    expect(byClass('test-editor')).toBe(editor)
    expect(editor.props['data-source']).toBe(true)
    expect(byClass('editor-pane').style.display).not.toBe('none')
    expect(byClass('preview-pane').style.display).toBe('none')
    await select('preview')
    expect(byClass('editor-pane').style.display).toBe('none')
    expect(byClass('preview-pane').style.display).not.toBe('none')
    await select('editor_view')
    expect(byClass('test-editor')).toBe(editor)
    expect(editor.props['data-source']).toBe(false)
  } finally { view.unmount() }
})
