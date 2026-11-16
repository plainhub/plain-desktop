import { expect, it, vi } from 'vitest'
import VPagination from '@/components/VPagination.vue'
import { click, clientTemplate, content, nodes, renderView } from '../helpers/vue-node-renderer'

clientTemplate(VPagination, 'src/components/VPagination.vue')

it('gives all numbered links the adaptive button style without changing their full label or navigation', () => {
  const go = vi.fn()
  const view = renderView(VPagination, { total: Number.MAX_SAFE_INTEGER, page: 10_000, limit: 1, go })
  try {
    const buttons = nodes(view.root).filter(node => node.tag === 'button')
    const numbers = buttons.filter(button => /^\d+$/.test(content(button).trim()))
    const last = numbers.at(-1)!
    expect(content(last)).toBe(String(Number.MAX_SAFE_INTEGER))
    expect(numbers.every(button => String(button.props.class).split(' ').includes('page-number'))).toBe(true)
    expect(buttons.filter(button => !numbers.includes(button)).every(button => !String(button.props.class).includes('page-number'))).toBe(true)
    click(last)
    expect(go).toHaveBeenCalledExactlyOnceWith(Number.MAX_SAFE_INTEGER)
  } finally {
    view.unmount()
  }
})
