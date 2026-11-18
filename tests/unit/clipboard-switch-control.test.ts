import { expect, it, vi } from 'vitest'
import VSwitch from '@/components/base/VSwitch.vue'
import { clientTemplate, fire, nodes, renderView } from '../helpers/vue-node-renderer'

clientTemplate(VSwitch, 'src/components/base/VSwitch.vue')

it('keeps the visible native switch checked until the authoritative prop changes', () => {
  const onChange = vi.fn()
  const view = renderView(VSwitch, { checked: true, label: 'Clipboard', deferChange: true, onChange })
  try {
    const input = nodes(view.root).find(node => node.tag === 'input')!
    expect(input.props['aria-label']).toBe('Clipboard')
    const browserToggle = { checked: false }
    fire(input, 'onChange', { target: browserToggle })
    expect(onChange).toHaveBeenCalledWith(false)
    expect(browserToggle.checked).toBe(true)
  } finally { view.unmount() }
})
