import { expect, it } from 'vitest'
import NoteModeToggle from '@/views/notes/NoteModeToggle.vue'
import { click, clientTemplate, nodes, renderView } from '../helpers/vue-node-renderer'

it('offers all three note modes and emits the selected mode', () => {
  const chosen: string[] = []
  const component = clientTemplate(NoteModeToggle, 'src/views/notes/NoteModeToggle.vue')
  const view = renderView(component, { mode: 'source', 'onUpdate:mode': (mode: string) => chosen.push(mode) })
  try {
    const buttons = nodes(view.root).filter(node => node.tag === 'button')
    expect(buttons.map(button => button.props['aria-label'])).toEqual(['editor_view', 'markdown_source', 'preview'])
    expect(buttons.map(button => button.props['aria-pressed'])).toEqual([false, true, false])
    for (const button of buttons) click(button)
    expect(chosen).toEqual(['editor', 'source', 'preview'])
  } finally { view.unmount() }
})
