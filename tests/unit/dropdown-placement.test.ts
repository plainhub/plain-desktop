import { afterEach, expect, it, vi } from 'vitest'
import { h, ref } from 'vue'
import VDropdown from '@/components/base/VDropdown.vue'
import { click, clientTemplate, flush, nodes, renderView } from '../helpers/vue-node-renderer'

const Dropdown = clientTemplate({ ...VDropdown }, 'src/components/base/VDropdown.vue')

afterEach(() => vi.unstubAllGlobals())

it.each([
  { name: 'prefers below despite room beside and more room above', left: 400, top: 370, expected: { left: '232px', top: '410px' } },
  { name: 'flips above near the bottom', left: 400, top: 540, expected: { left: '232px', bottom: '68px', maxHeight: '400px' } },
  { name: 'clamps at the left edge', left: 24, top: 100, expected: { left: '16px', top: '140px' } },
  { name: 'clamps at the right edge', left: 760, top: 100, expected: { left: '584px', top: '140px' } },
])('$name', async ({ left, top, expected }) => {
  vi.stubGlobal('document', new EventTarget())
  vi.stubGlobal('window', Object.assign(new EventTarget(), { innerWidth: 800, innerHeight: 600 }))
  const open = ref(false)
  const { root, portal, unmount } = renderView({
    render: () => h(Dropdown, { modelValue: open.value, 'onUpdate:modelValue': (value: boolean) => { open.value = value } }, {
      trigger: () => h('button', 'Actions'),
      default: () => h('button', 'Add to Bookmarks'),
    }),
  }, {}, {}, element => {
    Object.assign(element, { getBoundingClientRect: () => ({ width: 200, height: 100 }), scrollHeight: 100, offsetHeight: 100, clientHeight: 100 })
  })
  try {
    const container = nodes(root)[1]!
    Object.assign(container, { getBoundingClientRect: () => ({ left, right: left + 32, top, bottom: top + 32 }) })
    click(container.children[0]!)
    await flush()
    expect(nodes(portal).find(element => element.tag === 'div')?.props.style).toMatchObject(expected)
  } finally {
    unmount()
  }
})
