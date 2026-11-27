import { afterEach, expect, it, vi } from 'vitest'
import MediaThumbnail from '@/components/media/MediaThumbnail.vue'
import { clientTemplate, nodes, renderView, type Node } from '../helpers/vue-node-renderer'

afterEach(() => vi.unstubAllGlobals())

it('requests a thumbnail once it enters view and releases unmounted thumbnails', () => {
  const observed = new Set<Node>()
  let notify!: (entries: { target: Node; isIntersecting: boolean }[]) => void
  vi.stubGlobal('IntersectionObserver', class {
    constructor(callback: (entries: Parameters<typeof notify>[0], observer: { unobserve(image: Node): void }) => void) {
      notify = entries => callback(entries, this)
    }
    observe(image: Node) { observed.add(image) }
    unobserve(image: Node) { observed.delete(image) }
  })
  const component = clientTemplate(MediaThumbnail, 'src/components/media/MediaThumbnail.vue')
  function mount(src: string) {
    const view = renderView(component, { src }, {}, element => {
      Object.defineProperties(element, {
        dataset: { get: () => ({ src: element.props['data-src'] }) },
        src: { set: (value: string) => { element.props.src = value } },
      })
    })
    return { view, image: nodes(view.root).find(node => node.tag === 'img')! }
  }
  const first = mount('/visible.png')
  const second = mount('/below-fold.png')
  expect([first.image.props.src, second.image.props.src]).toEqual([undefined, undefined])
  expect(observed.size).toBe(2)
  notify([{ target: first.image, isIntersecting: true }, { target: second.image, isIntersecting: false }])
  expect(first.image.props.src).toBe('/visible.png')
  expect(second.image.props.src).toBeUndefined()
  expect(observed.has(first.image)).toBe(false)
  notify([{ target: second.image, isIntersecting: true }])
  expect(second.image.props.src).toBe('/below-fold.png')
  const hidden = mount('/removed.png')
  hidden.view.unmount()
  expect(hidden.image.props.src).toBeUndefined()
  expect(observed.size).toBe(0)
  second.view.unmount()
  first.view.unmount()
})
