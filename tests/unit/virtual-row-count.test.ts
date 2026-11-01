import { expect, it, vi } from 'vitest'
import Virtual from '@/components/virtualscroll/virtual'

it('renders viewport coverage and scroll buffers instead of one hundred file rows, and adapts to resize', async () => {
  const window = Object.assign(new EventTarget(), { innerWidth: 1200, innerHeight: 480 })
  vi.stubGlobal('window', window)
  try {
    const { useVirtualRowCount } = await import('@/hooks/device')
    const count = useVirtualRowCount(80)
    const list = new Virtual({
      keeps: count.value, estimateSize: 80, buffer: Math.round(count.value / 3),
      slotHeaderSize: 0, slotFooterSize: 0,
      uniqueIds: Array.from({ length: 1000 }, (_, index) => String(index)),
    }, vi.fn())
    expect(list.getRange().end - list.getRange().start + 1).toBe(18)
    list.handleScroll(40000)
    const range = list.getRange()
    expect(range.start * 80).toBeLessThanOrEqual(40000)
    expect((range.end + 1) * 80).toBeGreaterThanOrEqual(40480)
    window.innerHeight = 960
    window.dispatchEvent(new Event('resize'))
    list.updateParam('keeps', count.value)
    list.updateParam('buffer', Math.round(count.value / 3))
    list.handleSlotSizeChange()
    expect(list.getRange().end - list.getRange().start + 1).toBeLessThanOrEqual(38)
    expect((list.getRange().end + 1) * 80).toBeGreaterThanOrEqual(40960)
    list.destroy()
  } finally { vi.unstubAllGlobals() }
})
