import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import { defineComponent, h } from 'vue'

import ViewerPanel from '@/views/text-file/ViewerPanel.vue'

describe('ViewerPanel', () => {
  it('panel-body scrolls overflowing content (markdown preview has no own scroller)', async () => {
    const Tall = defineComponent({
      setup() {
        return () => h('div', { style: 'height: 5000px' }, 'tall content')
      },
    })
    const w = mount(ViewerPanel, {
      props: { title: 'Preview' },
      slots: { default: () => h(Tall) },
      // Author styles only resolve on rendered (document-attached) elements.
      attachTo: document.body,
    })
    // SFC styles are injected asynchronously in the browser test harness.
    await new Promise((r) => setTimeout(r, 50))
    const body = w.find('.panel-body')
    const el = body.element as HTMLElement
    // `getComputedStyle` must come from the element's own window — the test
    // global binds to the orchestrator document, which returns empty values.
    const style = el.ownerDocument.defaultView!.getComputedStyle(el)
    expect(style.overflowY).toBe('auto')
    expect(el.scrollHeight).toBeGreaterThanOrEqual(5000)
    w.unmount()
  })

  it('renders header title, meta and action slot', () => {
    const w = mount(ViewerPanel, {
      props: { title: 'Preview', meta: 'Markdown' },
      slots: { actions: () => h('button', 'copy') },
    })
    expect(w.text()).toContain('Preview')
    expect(w.find('.meta').text()).toBe('Markdown')
    expect(w.find('button').text()).toBe('copy')
    w.unmount()
  })
})
