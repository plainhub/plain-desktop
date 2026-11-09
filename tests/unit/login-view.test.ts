import { readFileSync, existsSync } from 'node:fs'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import * as Vue from 'vue'
import { createRenderer, nextTick, ssrContextKey } from 'vue'
import { compileScript, compileTemplate, parse } from 'vue/compiler-sfc'

const mock = vi.hoisted(() => ({ init: vi.fn(), startPairing: vi.fn(), requestInit: vi.fn(), push: vi.fn() }))
vi.mock('@/plugins/router', () => ({ default: { push: mock.push, currentRoute: { value: { query: {} } } } }))
vi.mock('@/lib/api/init', () => ({ requestInit: mock.requestInit }))
vi.mock('@/lib/api/api', () => ({ getApiHost: () => 'device.example', setPendingLoginDevice: vi.fn() }))
vi.mock('@/components/AuthShell.vue', () => ({ default: { setup: (_: unknown, { slots }: any) => () => slots.default?.() } }))
vi.mock('@/views/login/LoginForm.vue', () => ({
  default: { setup(_: unknown, { expose }: any) { expose({ init: mock.init, startPairing: mock.startPairing }); return () => null } },
}))
import LoginView from '@/views/login/LoginView.vue'
import source from '@/views/login/LoginView.vue?raw'

const { descriptor } = parse(source)
const template = compileTemplate({
  source: descriptor.template!.content,
  filename: 'LoginView.vue',
  id: 'login-view',
  compilerOptions: { mode: 'function', prefixIdentifiers: true, bindingMetadata: compileScript(descriptor, { id: 'login-view' }).bindings },
})
LoginView.render = new Function('Vue', template.code)(Vue)

type Node = { children: Node[]; parent?: Node }
const node = (): Node => ({ children: [] })
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
  patchProp: vi.fn(),
})

beforeEach(() => {
  vi.clearAllMocks()
  vi.stubGlobal('__IS_TAURI__', false)
  vi.stubGlobal('window', { location: { host: 'device.example' } })
})
afterEach(() => vi.unstubAllGlobals())

it.each([
  { authenticated: true, passwordRequired: false },
  { authenticated: false, passwordRequired: false },
  { authenticated: false, passwordRequired: true },
  { authenticated: false, passwordRequired: false, needsSetup: true },
])('initializes the login form directly for %j', async state => {
  const result = { status: 200, data: { signaturePublicKey: 'trusted-key', ...state } }
  mock.requestInit.mockResolvedValue(result)
  const app = renderer.createApp(LoginView)
  app.provide(ssrContextKey, { modules: new Set() })
  app.mount(node())
  try {
    await nextTick()
    await nextTick()
    expect(mock.requestInit).toHaveBeenCalledOnce()
    expect(mock.init).toHaveBeenCalledWith(result)
    expect(mock.push).not.toHaveBeenCalled()
    expect(mock.startPairing).not.toHaveBeenCalled()
  } finally { app.unmount() }
})

it('starts native pairing without requesting browser initialization', async () => {
  vi.stubGlobal('__IS_TAURI__', true)
  const app = renderer.createApp(LoginView)
  app.provide(ssrContextKey, { modules: new Set() })
  app.mount(node())
  try {
    await nextTick()
    expect(mock.startPairing).toHaveBeenCalledOnce()
    expect(mock.requestInit).not.toHaveBeenCalled()
    expect(mock.init).not.toHaveBeenCalled()
    expect(mock.push).not.toHaveBeenCalled()
  } finally { app.unmount() }
})

it('removes the setup route and retains the existing protected home fallback', () => {
  const router = readFileSync(new URL('../../src/plugins/router.ts', import.meta.url), 'utf8')
  expect(router).not.toContain("path: '/setup'")
  expect(existsSync(new URL('../../src/views/setup/SetupView.vue', import.meta.url))).toBe(false)
  expect(router).toContain("{ path: '/:pathMatch(.*)*', redirect: '/' }")
  expect(router).toMatch(/path: '\/',[\s\S]*?meta: \{ requiresAuth: true/)
  expect(router).toMatch(/if \(to\.meta\.requiresAuth && !canAccess\) \{[\s\S]*?path: '\/login'/)
  expect(router).toMatch(/path: '\/login',[\s\S]*?meta: \{ requiresAuth: false \}/)
})
