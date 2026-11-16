import { readFileSync } from 'node:fs'
import * as Vue from 'vue'
import { parse, compileScript, compileTemplate } from '@vue/compiler-sfc'
import { transpile } from 'typescript'

export const ModalShell = { setup(_: unknown, { slots }: any) { return () => Vue.h('section', [slots.headline?.(), slots.content?.(), slots.actions?.()]) } }
export const CopyButton = { props: { text: { type: String, required: true } }, render: () => Vue.h('button', 'copy') }

export function clientTemplate(component: any, filename: string) {
  const { descriptor } = parse(readFileSync(new URL(`../../${filename}`, import.meta.url), 'utf8'))
  const { code, errors } = compileTemplate({
    source: descriptor.template!.content, filename, id: filename,
    compilerOptions: { mode: 'function', prefixIdentifiers: true, isCustomElement: tag => tag.startsWith('i-'), bindingMetadata: compileScript(descriptor, { id: filename }).bindings },
  })
  if (errors.length) throw new Error(errors.join('\n'))
  component.render = new Function('Vue', transpile(code))(Vue)
  return component
}

export type Node = { tag: string; style: { display: string }; children: Node[]; parent?: Node; text?: string; props: Record<string, any> }
const node = (tag: string): Node => Object.assign({ tag, style: { display: '' }, children: [], props: {} }, { addEventListener() {} })
export const nodes = (element: Node): Node[] => [element, ...element.children.flatMap(nodes)]
export const content = (element: Node): string => [element.text ?? '', ...element.children.map(content)].join('')
export function fire(element: Node, eventName: string, event: unknown) {
  const callbacks = element.props[eventName]
  for (const callback of Array.isArray(callbacks) ? callbacks : [callbacks]) callback?.(event)
}
export function click(element: Node) { fire(element, 'onClick', { stopPropagation() {}, preventDefault() {} }) }
export async function flush() { for (let i = 0; i < 6; i++) { await Vue.nextTick(); await Promise.resolve() } }

export function renderView(component: any, props: Record<string, unknown> = {}, components: Record<string, any> = {}) {
  const header = node('header')
  const renderer = Vue.createRenderer<Node, Node>({
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
  const app = renderer.createApp(component, props)
  app.provide(Vue.ssrContextKey, { modules: new Set() })
  app.directive('tooltip', {})
  app.config.globalProperties.$t = (key: string) => key
  for (const [name, definition] of Object.entries(components)) app.component(name, definition)
  app.config.warnHandler = message => { throw new Error(message) }
  const root = node('root')
  app.mount(root)
  return { root, unmount: () => app.unmount() }
}
