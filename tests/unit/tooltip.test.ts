import { afterEach, expect, it, vi } from 'vitest'
import { createApp, type DirectiveBinding, type ObjectDirective, type VNode } from 'vue'
import tooltip from '@/plugins/tooltip'

afterEach(() => {
  vi.unstubAllGlobals()
  vi.useRealTimers()
})

it.each([
  { triggerWidth: 200, tooltipWidth: 20, left: '190px' },
  { triggerWidth: 20, tooltipWidth: 100, left: '60px' },
])('centers a $tooltipWidth px tooltip under a $triggerWidth px trigger', ({ triggerWidth, tooltipWidth, left }) => {
  vi.useFakeTimers()
  const tip = { style: {} as CSSStyleDeclaration, getBoundingClientRect: () => ({ width: tooltipWidth }) }
  vi.stubGlobal('document', { createElement: () => tip, body: { appendChild: vi.fn() }, getElementsByClassName: () => [] })
  const listeners = new Map<string, () => void>()
  const element = {
    tagName: 'SPAN',
    getBoundingClientRect: () => ({ x: 100, y: 50, left: 100, bottom: 74, width: triggerWidth }),
    addEventListener: (event: string, callback: () => void) => listeners.set(event, callback),
    removeEventListener: (event: string) => listeners.delete(event),
  } as unknown as HTMLElement
  const app = createApp({})
  app.use(tooltip)
  const directive = app.directive('tooltip') as ObjectDirective
  const binding = { value: 'Tip' } as DirectiveBinding
  const vnode = {} as VNode
  directive.mounted!(element, binding, vnode, null)
  try {
    listeners.get('mouseenter')!()
    vi.advanceTimersByTime(600)
    expect(tip.style.left).toBe(left)
    expect(tip.style.top).toBe('82px')
  } finally {
    directive.unmounted!(element, binding, vnode, null)
  }
})
