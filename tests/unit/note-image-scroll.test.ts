import { afterEach, expect, it, vi } from 'vitest'
import { EditorView } from '@codemirror/view'
import { ImageWidget } from '@/components/markdown-editor/widgets'

vi.mock('@/plugins/i18n', () => ({ i18n: { global: { t: (key: string) => key } } }))
vi.mock('@/lib/clipboard', () => ({ copyTextToClipboard: vi.fn() }))
vi.mock('@/components/markdown-editor/images', () => ({ resolveImageUrl: (link: string) => link }))

afterEach(() => vi.unstubAllGlobals())

it.each([0, 80, 320])('keeps clicked image source anchored %ipx below the viewport top', (offset) => {
  const elements: EventTarget[] = []
  vi.stubGlobal('document', { createElement: () => {
    const element = Object.assign(new EventTarget(), {
      dataset: {}, appendChild: vi.fn(),
      getBoundingClientRect: () => ({ top: 24 + offset }),
    })
    elements.push(element)
    return element
  } })
  const dispatch = vi.fn()
  const focus = vi.fn()
  const view = { dispatch, focus, scrollDOM: { getBoundingClientRect: () => ({ top: 24 }) } } as unknown as EditorView
  const source = new ImageWidget(123, 'fixture.svg', 'Synthetic image', view).toDOM()
  source.dispatchEvent(new Event('mousedown', { cancelable: true }))
  expect(dispatch).toHaveBeenCalledWith({
    selection: { anchor: 123 },
    effects: EditorView.scrollIntoView(123, { y: 'start', yMargin: offset }),
  })
  expect(focus).toHaveBeenCalledOnce()
  expect(elements).toHaveLength(2)
})
