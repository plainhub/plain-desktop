import { afterEach, expect, it, vi } from 'vitest'
import ClipboardHistory from '@/views/clipboard/ClipboardHistory.vue'
import ClipboardItem from '@/components/ClipboardItem.vue'
import VCopyButton from '@/components/base/VCopyButton.vue'
import VDropdown from '@/components/base/VDropdown.vue'
import { click, clientTemplate, content, fire, flush, nodes, renderView } from '../helpers/vue-node-renderer'

vi.mock('@/lib/format', () => ({
  formatTimeAgo: () => '15 minutes ago',
  formatDateTimeFull: () => 'November 22, 2026, 12:00',
}))

clientTemplate(ClipboardHistory, 'src/views/clipboard/ClipboardHistory.vue')
clientTemplate(ClipboardItem, 'src/components/ClipboardItem.vue')
clientTemplate(VCopyButton, 'src/components/base/VCopyButton.vue')
clientTemplate(VDropdown, 'src/components/base/VDropdown.vue')

const item = { id: 'record-id', label: 'Example label', source: 'source-id', text: 'Example clipboard text', createdAt: '2026-11-22T12:00:00.000Z', sensitive: false }

afterEach(() => vi.unstubAllGlobals())

function mount(label: string) {
  const writeText = vi.fn().mockResolvedValue(undefined)
  const onDelete = vi.fn()
  vi.stubGlobal('document', new EventTarget())
  vi.stubGlobal('window', Object.assign(new EventTarget(), { innerWidth: 800, innerHeight: 600 }))
  vi.stubGlobal('navigator', { clipboard: { writeText } })
  const view = renderView(ClipboardHistory, { items: [{ ...item, label }], onDelete }, { VCopyButton }, element => {
    Object.assign(element, { getBoundingClientRect: () => ({ width: 100, top: 16, right: 100, bottom: 48 }), scrollHeight: 32, offsetHeight: 32, clientHeight: 32 })
  })
  return { ...view, writeText, onDelete }
}

it.each([item.label, ''])('keeps the label and source out of the card header (label: %j)', label => {
  const view = mount(label)
  try {
    expect(content(view.root)).toBe('15 minutes ago' + item.text)
    expect(nodes(view.root).find(node => node.tag === 'time')?.props.datetime).toBe(item.createdAt)
  } finally { view.unmount() }
})

it('opens the raw record from the timestamp and keeps copy and delete working', async () => {
  const view = mount(item.label)
  try {
    const timestamp = nodes(view.root).find(node => node.props['aria-haspopup'] === 'dialog')!
    expect(timestamp?.tag).toBe('button')
    expect(timestamp.props['aria-expanded']).toBe(false)
    click(timestamp.parent!)
    await flush()
    expect(timestamp.props['aria-expanded']).toBe(true)

    const raw = nodes(view.portal).find(node => node.tag === 'pre')!
    expect(JSON.parse(content(raw))).toEqual(item)
    fire(raw, 'onKeydown', { key: 'Escape', stopPropagation() {} })
    await flush()
    expect(timestamp.props['aria-expanded']).toBe(false)
    expect(nodes(view.portal).some(node => node.tag === 'pre')).toBe(false)

    const action = (name: string) => nodes(view.root).find(node => node.props['aria-label'] === name)!
    click(action('copy'))
    await flush()
    expect(view.writeText).toHaveBeenCalledExactlyOnceWith(item.text)
    click(action('delete'))
    expect(view.onDelete).toHaveBeenCalledExactlyOnceWith(item)
  } finally { view.unmount() }
})
