import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import * as Vue from 'vue'
import { createRenderer, nextTick, reactive, ssrContextKey } from 'vue'
import { compileScript, compileTemplate, parse } from '@vue/compiler-sfc'
import source from '@/components/HttpSecurityNotice.vue?raw'
import en from '@/locales/en-US/common'
import zhCN from '@/locales/zh-CN/common'
import zhTW from '@/locales/zh-TW/common'
import { SUPPORTED_LOCALES } from '@/plugins/i18n'
vi.mock('@/lib/prefs/appearance', () => ({ get: () => '' }))
const localeMessages = import.meta.glob<{ default: Record<string, unknown> }>('../../src/locales/*/index.ts', { eager: true })
const noticeKeys = ['http_security_warning', 'http_security_switch', 'http_security_certificate'] as const
const state = vi.hoisted(() => ({ request: vi.fn(), apiBase: 'http://example.test:8080' }))
vi.mock('@/lib/api/api', () => ({ getApiBaseUrl: () => state.apiBase }))
vi.mock('@/lib/api/init', () => ({ requestInit: state.request }))
const route = reactive({ fullPath: '/files?path=%2Fphotos#details' })
vi.mock('vue-router', () => ({ useRoute: () => route }))
import HttpSecurityNotice from '@/components/HttpSecurityNotice.vue'

const { descriptor } = parse(source, { filename: 'HttpSecurityNotice.vue' })
const template = compileTemplate({
  source: descriptor.template!.content, filename: 'HttpSecurityNotice.vue', id: 'notice',
  compilerOptions: { mode: 'function', prefixIdentifiers: true, bindingMetadata: compileScript(descriptor, { id: 'notice' }).bindings },
})
HttpSecurityNotice.render = new Function('Vue', template.code)(Vue)

type Element = { tag: string; props: Record<string, unknown>; children: Element[]; parent?: Element; text?: string }
const node = (tag: string): Element => ({ tag, props: {}, children: [] })
function mount(messages: Record<string, unknown> = en) {
  const renderer = createRenderer<Element, Element>({
    createElement: node, createText: text => ({ ...node('#text'), text }), createComment: () => node('comment'),
    insert(child, parent, anchor) {
      if (child.parent) child.parent.children.splice(child.parent.children.indexOf(child), 1)
      child.parent = parent
      parent.children.splice(anchor ? parent.children.indexOf(anchor) : parent.children.length, 0, child)
    },
    remove(child) { if (child.parent) child.parent.children = child.parent.children.filter(item => item !== child) },
    setText(element, text) { element.text = text }, setElementText(element, text) { element.text = text }, parentNode: child => child.parent ?? null,
    nextSibling: child => child.parent?.children[child.parent.children.indexOf(child) + 1] ?? null, patchProp(element, key, _previous, value) { element.props[key] = value },
  })
  const root = node('root')
  const app = renderer.createApp(HttpSecurityNotice)
  app.config.globalProperties.$t = (key: string) => messages[key] ?? key
  app.provide(ssrContextKey, { modules: new Set() })
  app.mount(root)
  return { root, app }
}
const textContent = (element: Element): string => (element.text ?? '') + element.children.map(textContent).join('')
const flush = async () => { await Promise.resolve(); await nextTick() }
beforeEach(() => {
  vi.stubGlobal('__IS_TAURI__', false)
  vi.stubGlobal('window', { location: { protocol: 'http:', origin: 'http://example.test:8080' } })
  state.apiBase = 'http://example.test:8080'
  state.request.mockReset().mockResolvedValue({ status: 200, data: { httpsPort: 9443 } })
  route.fullPath = '/files?path=%2Fphotos#details'
})
afterEach(() => vi.unstubAllGlobals())

it('shows warning immediately and adds the link after init, following route changes', async () => {
  let resolve!: (value: unknown) => void
  state.request.mockImplementationOnce(() => new Promise(done => { resolve = done }))
  const { root, app } = mount()
  const notice = root.children[0]
  expect(notice.tag).toBe('div')
  expect(notice.children[0].children.some(child => child.tag === 'a')).toBe(false)
  resolve({ status: 200, data: { httpsPort: 9443 } })
  await flush()
  const link = notice.children[0].children.find(child => child.tag === 'a')!
  expect(link.props.href).toBe('https://example.test:9443/files?path=%2Fphotos#details')
  expect(link.props.rel).toBe('noreferrer')
  route.fullPath = '/images?page=2'
  await nextTick()
  expect(link.props.href).toBe('https://example.test:9443/images?page=2')
  route.fullPath = '//evil.example/files'
  await nextTick()
  expect(link.props.href).toBe('https://example.test:9443//evil.example/files')
  app.unmount()
})

it.each([undefined, 0, -1, 70000, '9443'])('keeps warning without a link for port %s', async httpsPort => {
  state.request.mockResolvedValueOnce({ status: 200, data: { httpsPort } })
  const { root, app } = mount()
  await flush()
  expect(root.children[0].tag).toBe('div')
  expect(root.children[0].children[0].children.some(child => child.tag === 'a')).toBe(false)
  app.unmount()
})

it('keeps the warning when init fails', async () => {
  state.request.mockRejectedValueOnce(new Error('offline'))
  const { root, app } = mount()
  await flush()
  expect(root.children[0].tag).toBe('div')
  expect(root.children[0].children[0].children.some(child => child.tag === 'a')).toBe(false)
  app.unmount()
})

it.each([['https:', false], ['http:', true]])('hides the notice and skips init on %s native=%s', (protocol, nativeApp) => {
  vi.stubGlobal('__IS_TAURI__', nativeApp)
  vi.stubGlobal('window', { location: { protocol, origin: `${protocol}//example.test` } })
  const { root, app } = mount()
  expect(root.children.some(child => child.tag === 'div')).toBe(false)
  expect(state.request).not.toHaveBeenCalled()
  app.unmount()
})

it.each(['http://peer.test:8080', 'http://example.test:8081', 'https://example.test:8080'])('keeps the warning without querying a different API origin %s', async apiBase => {
  state.apiBase = apiBase
  const { root, app } = mount()
  await flush()
  expect(root.children[0].tag).toBe('div')
  expect(root.children[0].children[0].children.some(child => child.tag === 'a')).toBe(false)
  expect(state.request).not.toHaveBeenCalled()
  app.unmount()
})

it.each([
  [en, 'File uploads and downloads over HTTP are not encrypted.'],
  [zhCN, 'HTTP 下文件上传和下载未加密。'],
  [zhTW, 'HTTP 下檔案上傳和下載未加密。'],
])('limits the warning to file uploads and downloads', async (messages, expected) => {
  const { root, app } = mount(messages)
  await flush()
  const spans = root.children[0].children.filter(child => child.tag === 'span')
  expect(spans).toHaveLength(2)
  expect(spans[0].children.some(child => child.tag === 'a')).toBe(true)
  expect(textContent(spans[0])).toContain(expected)
  app.unmount()
})

it.each(SUPPORTED_LOCALES)('provides and renders every notice message in $code without fallback', async ({ code }) => {
  const messages = localeMessages[`../../src/locales/${code}/index.ts`]?.default
  expect(messages).toBeDefined()
  for (const key of noticeKeys) {
    expect(Object.hasOwn(messages, key), `${code}: ${key}`).toBe(true)
    expect(typeof messages[key]).toBe('string')
    expect((messages[key] as string).trim().length).toBeGreaterThan(0)
    if (code !== 'en-US') expect(messages[key]).not.toBe(en[key])
  }
  const { root, app } = mount(messages)
  await flush()
  const notice = root.children[0]
  const spans = notice.children.filter(child => child.tag === 'span')
  expect(spans[0].children.find(child => child.tag === 'a')?.text).toBe(messages.http_security_switch)
  expect(textContent(spans[0])).toContain(messages.http_security_warning)
  expect(spans[1].text).toBe(messages.http_security_certificate)
  app.unmount()
})

// Keep this notice owned by the login route, not the global application shell.
it('mounts the HTTP security notice only on the login page', () => {
  const views = import.meta.glob<string>('../../src/**/*.vue', { eager: true, query: '?raw', import: 'default' })
  const owners = Object.entries(views).filter(([, source]) => /<HttpSecurityNotice[\s/>]/.test(source)).map(([path]) => path)
  expect(owners).toEqual(['../../src/views/login/LoginView.vue'])
  expect(views[owners[0]]).toMatch(/<\/AuthShell>\s*<HttpSecurityNotice \/>/)
  expect(source).not.toContain('<footer')
  expect(source).not.toContain('margin-top: auto')
})
