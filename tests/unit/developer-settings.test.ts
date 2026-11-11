import { expect, it, vi } from 'vitest'
import * as Vue from 'vue'
import { createRenderer, h, nextTick, reactive, ssrContextKey } from 'vue'
import { compileScript, compileTemplate, parse } from 'vue/compiler-sfc'

const fixture = vi.hoisted(() => ({ target: null as any }))
vi.mock('@/stores/temp', () => ({ useTempStore: () => fixture.target }))
vi.mock('@/stores/main', () => ({ useMainStore: () => ({}) }))
vi.mock('@/plugins/router', () => ({
  default: { currentRoute: { value: { path: '/settings/device-info' } } },
  replacePath: vi.fn(),
}))
import SettingsSidebar from '@/views/settings/SettingsSidebar.vue'
import source from '@/views/settings/SettingsSidebar.vue?raw'

const { descriptor } = parse(source)
const template = compileTemplate({
  source: descriptor.template!.content,
  filename: 'SettingsSidebar.vue',
  id: 'settings-sidebar',
  compilerOptions: {
    mode: 'function', prefixIdentifiers: true,
    bindingMetadata: compileScript(descriptor, { id: 'settings-sidebar' }).bindings,
    isCustomElement: tag => tag.startsWith('i-lucide:'),
  },
})
SettingsSidebar.render = new Function('Vue', template.code)(Vue)

type Node = { children: Node[]; props: Record<string, unknown>; parent?: Node }
const node = (): Node => ({ children: [], props: {} })
function remove(child: Node) {
  const siblings = child.parent?.children
  if (siblings) siblings.splice(siblings.indexOf(child), 1)
  child.parent = undefined
}
const renderer = createRenderer<Node, Node>({
  insert(child, parent, anchor) {
    remove(child)
    child.parent = parent
    parent.children.splice(anchor ? parent.children.indexOf(anchor) : parent.children.length, 0, child)
  },
  remove, createElement: node, createText: node, createComment: node,
  setText: vi.fn(), setElementText: vi.fn(), parentNode: child => child.parent ?? null,
  nextSibling: child => child.parent?.children[child.parent.children.indexOf(child) + 1] ?? null,
  patchProp: (element, key, _old, value) => { element.props[key] = value },
})
function titles(root: Node): unknown[] {
  return [root.props.title, ...root.children.flatMap(titles)].filter(Boolean)
}

it('shows developer entries only while the target enables developer mode', async () => {
  fixture.target = reactive({ app: { developerMode: false } })
  const app = renderer.createApp(SettingsSidebar)
  app.component('LeftSidebar', { setup: (_, { slots }) => () => h('aside', slots.body?.()) })
  app.component('SidebarListItem', { props: ['title', 'active'], setup: props => () => h('li', { title: props.title }) })
  app.config.globalProperties.$t = (key: string) => key
  app.provide(ssrContextKey, { modules: new Set() })
  const root = node()
  app.mount(root)
  try {
    for (const enabled of [false, true, false]) {
      fixture.target.app.developerMode = enabled
      await nextTick()
      const visible = titles(root)
      expect(visible).toContain('device_info')
      expect(visible).toContain('events')
      for (const entry of ['developer.prefs', 'developer.database', 'developer.logs']) {
        expect(visible.includes(entry)).toBe(enabled)
      }
    }
  } finally { app.unmount() }
})
