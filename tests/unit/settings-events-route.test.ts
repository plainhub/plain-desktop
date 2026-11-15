import { afterEach, expect, it, vi } from 'vitest'
import { readFileSync } from 'node:fs'
import * as Vue from 'vue'
import { parse, compileScript, compileTemplate } from '@vue/compiler-sfc'
import { transpile } from 'typescript'
import { createRenderer, h, KeepAlive, nextTick, reactive, ref, ssrContextKey } from 'vue'
import { encodeBase64 } from '@/lib/strutil'
import SettingsEventsView from '@/views/settings/SettingsEventsView.vue'

const fixture = vi.hoisted(() => ({ route: null as any, read: vi.fn(), context: null as any }))
vi.mock('vue-router', () => ({ useRoute: () => fixture.route }))
vi.mock('@/lib/api/gql-client', () => ({ getGraphqlContextKey: () => fixture.context.value }))
vi.mock('@/views/settings/events/api', () => ({ readEvents: (...args: unknown[]) => fixture.read(...args) }))
vi.mock('@/views/settings/events/EventCard.vue', () => ({ default: { render: () => null } }))
vi.mock('@/lib/format', () => ({ formatDate: (value: string) => value.slice(0, 10) }))

const { descriptor } = parse(readFileSync(new URL('../../src/views/settings/SettingsEventsView.vue', import.meta.url), 'utf8'))
const { code, errors } = compileTemplate({
  source: descriptor.template!.content,
  filename: 'SettingsEventsView.vue',
  id: 'settings-events-route',
  compilerOptions: {
    mode: 'function', prefixIdentifiers: true,
    bindingMetadata: compileScript(descriptor, { id: 'settings-events-route' }).bindings,
  },
})
if (errors.length) throw new Error(errors.join('\n'))
SettingsEventsView.render = new Function('Vue', transpile(code))(Vue)

type Node = { tag: string; children: Node[]; parent?: Node; text?: string; props: Record<string, any>; scrollTop: number; clientHeight: number; scrollHeight: number }
const node = (tag: string): Node => ({ tag, children: [], props: {}, scrollTop: 0, clientHeight: 600, scrollHeight: 2000 })
const mounted: Array<() => void> = []
afterEach(() => { mounted.splice(0).forEach(unmount => unmount()); fixture.read.mockReset() })

function mount() {
  const header = node('header')
  const renderer = createRenderer<Node, Node>({
    createElement: node, createText: text => ({ ...node('#text'), text }), createComment: () => node('#comment'),
    insert(child, parent, anchor) {
      if (child.parent) child.parent.children.splice(child.parent.children.indexOf(child), 1)
      child.parent = parent
      parent.children.splice(anchor ? parent.children.indexOf(anchor) : parent.children.length, 0, child)
    },
    remove(child) { if (child.parent) child.parent.children.splice(child.parent.children.indexOf(child), 1) },
    setText(child, text) { child.text = text }, setElementText(child, text) { child.text = text },
    parentNode: child => child.parent ?? null,
    nextSibling: child => child.parent?.children[child.parent.children.indexOf(child) + 1] ?? null,
    patchProp: (element, key, _previous, value) => { element.props[key] = value }, querySelector: () => header,
  })
  const visible = ref(true)
  const app = renderer.createApp({ render: () => h(KeepAlive, null, { default: () => visible.value ? h(SettingsEventsView) : null }) })
  app.provide(ssrContextKey, { modules: new Set() })
  app.directive('tooltip', {})
  app.config.globalProperties.$t = (key: string) => key
  app.config.warnHandler = message => { if (!message.startsWith('Failed to resolve component:')) throw new Error(message) }
  const root = node('root')
  app.mount(root)
  mounted.push(() => app.unmount())
  return { root, visible }
}
async function flush() { await nextTick(); await Promise.resolve(); await nextTick() }

it('reads header search from the route once on activation, resets pagination and refreshes for target changes', async () => {
  fixture.route = reactive({ path: '/settings/events', query: { q: encodeBase64('text:"192.0.2.24"') } })
  fixture.context = ref('first')
  fixture.read.mockResolvedValue([])
  const { root, visible } = mount()
  await flush()
  expect(fixture.read.mock.calls).toEqual([['192.0.2.24', 0, 50]])
  const tags = (element: Node): string[] => [element.tag, ...element.children.flatMap(tags)]
  expect(tags(root)).not.toContain('v-text-field')
  expect(tags(root)).not.toContain('h1')
  fixture.route.query.q = encodeBase64('text:"Chrome"')
  await flush()
  expect(fixture.read).toHaveBeenLastCalledWith('Chrome', 0, 50)
  fixture.context.value = 'second'
  await flush()
  expect(fixture.read).toHaveBeenCalledTimes(3)
  visible.value = false
  await flush()
  fixture.route.query.q = ''
  await flush()
  expect(fixture.read).toHaveBeenCalledTimes(3)
  visible.value = true
  await flush()
  expect(fixture.read).toHaveBeenLastCalledWith('', 0, 50)
  expect(fixture.read).toHaveBeenCalledTimes(4)
})

function find(element: Node, predicate: (element: Node) => boolean): Node | undefined {
  if (predicate(element)) return element
  for (const child of element.children) {
    const match = find(child, predicate)
    if (match) return match
  }
}
const event = (id: number) => ({ id: String(id), type: 'LOGIN', createdAt: id < 25 ? '2026-11-15T10:00:00Z' : '2026-11-14T10:00:00Z' })

it('loads on scrolling near the bottom, serializes requests and stops after the last page', async () => {
  fixture.route = reactive({ path: '/settings/events', query: { q: '' } })
  fixture.context = ref('first')
  let finish!: (rows: unknown[]) => void
  fixture.read.mockResolvedValueOnce(Array.from({ length: 50 }, (_, index) => event(index)))
    .mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))
  const { root } = mount()
  await flush()
  const scroller = find(root, element => element.props.class === 'scroll-content events-view')!
  expect(fixture.read).toHaveBeenCalledTimes(1)
  expect(find(root, element => element.tag === 'v-filled-button')).toBeUndefined()
  expect(find(root, element => element.tag === 'ol')?.children.filter(element => element.tag === 'li')).toHaveLength(50)
  scroller.scrollTop = 1250
  scroller.props.onScrollPassive()
  scroller.props.onScrollPassive()
  expect(fixture.read.mock.calls).toEqual([['', 0, 50], ['', 50, 50]])
  finish([event(50)])
  await flush()
  scroller.props.onScrollPassive()
  expect(fixture.read).toHaveBeenCalledTimes(2)
  expect(find(root, element => element.props.class === 'event-date subtle')?.text).toBe('2026-11-15')
})

it('fills a short viewport automatically and preserves errors without retry loops', async () => {
  fixture.route = reactive({ path: '/settings/events', query: { q: '' } })
  fixture.context = ref('first')
  fixture.read.mockResolvedValueOnce(Array.from({ length: 50 }, (_, index) => event(index)))
    .mockRejectedValueOnce(new Error('unavailable'))
  const { root, visible } = mount()
  const scroller = find(root, element => element.props.class === 'scroll-content events-view')!
  scroller.scrollHeight = 700
  await flush()
  await flush()
  expect(fixture.read.mock.calls).toEqual([['', 0, 50], ['', 50, 50]])
  expect(find(root, element => element.props.role === 'alert')?.text).toBe('unavailable')
  scroller.props.onScrollPassive()
  expect(fixture.read).toHaveBeenCalledTimes(2)
  visible.value = false
  await flush()
  scroller.props.onScrollPassive()
  expect(fixture.read).toHaveBeenCalledTimes(2)
})
