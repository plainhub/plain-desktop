import { readFileSync } from 'node:fs'
import { afterEach, expect, it, vi } from 'vitest'
import * as Vue from 'vue'
import { createRenderer, nextTick, ssrContextKey } from 'vue'
import { parse, compileScript, compileTemplate } from '@vue/compiler-sfc'
import { transpile } from 'typescript'
import EventCard from '@/views/settings/events/EventCard.vue'
import VDropdown from '@/components/base/VDropdown.vue'
import type { EventRecord } from '@/views/settings/events/api'
import messages from '@/locales/en-US/events'
import { content } from '../helpers/vue-node-renderer'

vi.mock('@/lib/format', () => ({ formatTime: () => '23:24', formatDateTimeFull: (value: string) => `Full ${value}` }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => messages[key as keyof typeof messages] ?? key }) }))
for (const [component, path] of [[EventCard, 'views/settings/events/EventCard.vue'], [VDropdown, 'components/base/VDropdown.vue']] as const) {
  const { descriptor } = parse(readFileSync(new URL(`../../src/${path}`, import.meta.url), 'utf8'))
  const template = compileTemplate({ source: descriptor.template!.content, filename: path, id: path,
    compilerOptions: { mode: 'function', prefixIdentifiers: true, bindingMetadata: compileScript(descriptor, { id: path }).bindings } })
  if (template.errors.length) throw new Error(template.errors.join('\n'))
  component.render = new Function('Vue', transpile(template.code))(Vue)
}

type Element = { tag: string; props: Record<string, any>; children: Element[]; parent?: Element; text?: string; focus: ReturnType<typeof vi.fn>; getBoundingClientRect: () => { top: number; bottom: number; left: number; right: number; width: number; height: number }; contains: (target: Element) => boolean }
function node(tag: string): Element {
  const result: Element = { tag, props: {}, children: [], focus: vi.fn(),
    getBoundingClientRect: () => ({ top: 100, bottom: 132, left: 100, right: 240, width: 140, height: 32 }),
    contains: target => target === result || result.children.some(child => child.contains(target)) }
  result.focus.mockImplementation(() => {
    let ancestor: Element | undefined = result
    result.props.focusedWhileVisible = true
    while (ancestor) {
      if ((ancestor.props.style as { visibility?: string } | undefined)?.visibility === 'hidden') result.props.focusedWhileVisible = false
      ancestor = ancestor.parent
    }
    result.props.onFocused?.()
  })
  return result
}
const cleanups: Array<() => void> = []
afterEach(() => { cleanups.splice(0).forEach(cleanup => cleanup()); vi.unstubAllGlobals() })
function mount() {
  const body = node('body')
  const listeners = new Map<string, Set<(event: any) => void>>()
  const dispatch = (event: { type: string }) => listeners.get(event.type)?.forEach(listener => listener(event))
  vi.stubGlobal('document', {
    addEventListener: (type: string, listener: (event: any) => void) => { if (!listeners.has(type)) listeners.set(type, new Set()); listeners.get(type)!.add(listener) },
    removeEventListener: (type: string, listener: (event: any) => void) => listeners.get(type)?.delete(listener), dispatchEvent: dispatch,
  })
  vi.stubGlobal('window', { innerWidth: 1024, innerHeight: 800, addEventListener: vi.fn(), removeEventListener: vi.fn() })
  const renderer = createRenderer<Element, Element>({
    createElement: node, createText: text => ({ ...node('#text'), text }), createComment: () => node('#comment'),
    insert(child, parent, anchor) { if (child.parent) child.parent.children.splice(child.parent.children.indexOf(child), 1); child.parent = parent; parent.children.splice(anchor ? parent.children.indexOf(anchor) : parent.children.length, 0, child) },
    remove(child) { if (child.parent) child.parent.children.splice(child.parent.children.indexOf(child), 1) },
    setText(child, text) { child.text = text }, setElementText(child, text) { child.text = text },
    parentNode: child => child.parent ?? null, nextSibling: child => child.parent?.children[child.parent.children.indexOf(child) + 1] ?? null,
    patchProp: (element, key, _previous, value) => { element.props[key] = value }, querySelector: () => body,
  })
  const entry: EventRecord = { id: 'event-fixture', type: 'LOGIN', data: { __typename: 'LoginEventData', sessionType: 'LOGIN' }, clientId: '706c6169-25a5-85e6-9135-e2e4263d8d01', clientName: '', clientIp: '192.0.2.24', appName: 'Chrome', appVersion: '142', osName: 'Windows', osVersion: '11', createdAt: '2026-11-15T06:24:00Z' }
  const root = node('root')
  const app = renderer.createApp(EventCard, { entry })
  app.provide(ssrContextKey, { modules: new Set() })
  app.directive('tooltip', { mounted: (element: Element, binding) => { element.props.tooltip = binding.value } })
  app.config.globalProperties.$t = (key: string) => messages[key as keyof typeof messages] ?? key
  app.mount(root)
  cleanups.push(() => app.unmount())
  return { root, body, entry, dispatch }
}
function find(element: Element, predicate: (element: Element) => boolean): Element | undefined {
  if (predicate(element)) return element
  for (const child of element.children) { const match = find(child, predicate); if (match) return match }
}
async function clickTitle(title: Element) {
  let element: Element | undefined = title
  while (element && !element.props.onClick) element = element.parent
  element!.props.onClick({ preventDefault() {}, stopPropagation() {} })
  await nextTick()
  await nextTick()
}

it('opens the complete raw event only from its title and dismisses by Escape or outside click', async () => {
  const { root, body, entry, dispatch } = mount()
  const title = find(root, element => element.props.role === 'button')!
  expect(title.tag).toBe('span')
  expect(title.props.tabindex).toBe('0')
  expect(find(root, element => element.tag === 'button')).toBeUndefined()
  expect(title.text).toBe('Signed in')
  expect(title.props['aria-expanded']).toBe(false)
  expect(find(body, element => element.tag === 'pre')).toBeUndefined()
  const time = find(root, element => element.tag === 'time')!
  const anchor = find(time, element => element.props.tooltip !== undefined)!
  expect(time.props.datetime).toBe(entry.createdAt)
  expect(time.props.tooltip).toBeUndefined()
  expect(anchor.tag).toBe('span')
  expect(content(anchor)).toBe('23:24')
  expect(anchor.props.tooltip).toBe(`Full ${entry.createdAt}`)
  expect(find(root, element => element.props.class === 'event-client')).toBeUndefined()
  await clickTitle(title)
  let raw = find(body, element => element.tag === 'pre')!
  expect(JSON.parse(raw.text!)).toEqual(entry)
  expect(title.props['aria-expanded']).toBe(true)
  await new Promise<void>(resolve => {
    if (raw.focus.mock.calls.length) resolve()
    else raw.props.onFocused = resolve
  })
  expect(raw.focus).toHaveBeenCalledTimes(1)
  expect(raw.props.focusedWhileVisible).toBe(true)
  raw.props.onKeydown({ key: 'Escape', stopPropagation() {} })
  await nextTick()
  expect(find(body, element => element.tag === 'pre')).toBeUndefined()
  expect(title.focus).toHaveBeenCalledTimes(1)
  await clickTitle(title)
  raw = find(body, element => element.tag === 'pre')!
  dispatch({ type: 'click', target: raw } as any)
  await nextTick()
  expect(find(body, element => element.tag === 'pre')).toBeDefined()
  dispatch({ type: 'click', target: root } as any)
  await nextTick()
  expect(find(body, element => element.tag === 'pre')).toBeUndefined()
  expect(title.props['aria-expanded']).toBe(false)
})
