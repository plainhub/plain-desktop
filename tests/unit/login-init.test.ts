import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import * as Vue from 'vue'
import { createRenderer, defineComponent, h, nextTick, ref, ssrContextKey } from 'vue'
import { readFileSync } from 'node:fs'
import { compileScript, compileTemplate, parse } from 'vue/compiler-sfc'
const mock = vi.hoisted(() => ({ token: vi.fn(), identity: vi.fn() }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('@/plugins/router', () => ({ default: {} }))
vi.mock('@/lib/api/api', () => ({ getApiBaseUrl: vi.fn(), getPendingLoginDevice: vi.fn(), clearPendingLoginDevice: vi.fn() }))
vi.mock('@/lib/device/current', () => ({ getCurrentAuthToken: mock.token, clearCurrentSession: vi.fn().mockResolvedValue(undefined) }))
vi.mock('@/lib/device/login-peers', () => ({ getBrowserIdentity: mock.identity, forgetBrowserIdentity: vi.fn(), replaceBrowserIdentity: vi.fn(), findLoginPeer: vi.fn(), saveBrowserIdentity: vi.fn(), saveBrowserSession: vi.fn(), preloadLoginPeers: vi.fn(), peerHost: vi.fn() }))
vi.mock('@/lib/device/client-id', () => ({ getDesktopClientId: vi.fn(), getRemoteClientId: vi.fn(), setRemoteClientId: vi.fn() }))
vi.mock('@/lib/api/http', () => ({ httpRequest: vi.fn() }))
vi.mock('@/lib/api/login-handshake', () => ({ performLoginHandshake: vi.fn() }))
vi.mock('@/lib/device/pairing', () => ({ createPairing: vi.fn() }))
vi.mock('@/lib/api/gql-client', () => ({ bindGraphqlClient: vi.fn() }))
import { createPairing, type PairingView } from '@/lib/device/pairing'
import { setRemoteClientId } from '@/lib/device/client-id'
import { saveBrowserIdentity, saveBrowserSession, replaceBrowserIdentity, forgetBrowserIdentity } from '@/lib/device/login-peers'
import { clearCurrentSession } from '../../src/lib/device/current'
import { performLoginHandshake } from '../../src/lib/api/login-handshake'
import { useLogin, getSafeRedirect } from '../../src/views/login/login'
import LoginForm from '@/views/login/LoginForm.vue'
import VFilledButton from '@/components/base/VFilledButton.vue'
import VOutlinedButton from '@/components/base/VOutlinedButton.vue'
import type { InitResult } from '@/lib/api/init'

for (const [component, path] of [
  [LoginForm, 'views/login/LoginForm.vue'],
  [VFilledButton, 'components/base/VFilledButton.vue'],
  [VOutlinedButton, 'components/base/VOutlinedButton.vue'],
] as const) {
  const { descriptor } = parse(readFileSync(new URL(`../../src/${path}`, import.meta.url), 'utf8'))
  const template = compileTemplate({
    source: descriptor.template!.content, filename: path, id: path,
    compilerOptions: {
      mode: 'function', prefixIdentifiers: true,
      isCustomElement: tag => tag.startsWith('i-material-symbols:'),
      bindingMetadata: compileScript(descriptor, { id: path }).bindings,
    },
  })
  component.render = new Function('Vue', template.code)(Vue)
}

type Element = { tag: string; props: Record<string, unknown>; style: { display: string }; children: Element[]; parent?: Element; text: string }
const element = (tag: string, text = ''): Element => ({ tag, props: {}, style: { display: '' }, children: [], text })
function mountLoginForm() {
  const renderer = createRenderer<Element, Element>({
    createElement: element, createText: text => element('#text', text), createComment: () => element('#comment'),
    insert(child, parent, anchor) {
      if (child.parent) child.parent.children.splice(child.parent.children.indexOf(child), 1)
      child.parent = parent
      parent.children.splice(anchor ? parent.children.indexOf(anchor) : parent.children.length, 0, child)
    },
    remove(child) { if (child.parent) child.parent.children = child.parent.children.filter(item => item !== child) },
    setText(node, text) { node.text = text }, setElementText(node, text) { node.text = text },
    parentNode: child => child.parent ?? null,
    nextSibling: child => child.parent?.children[child.parent.children.indexOf(child) + 1] ?? null,
    patchProp(node, key, _previous, value) { node.props[key] = value },
  })
  const root = element('root')
  const app = renderer.createApp(LoginForm)
  app.config.globalProperties.$t = (key: string) => key
  app.component('TouchPhone', defineComponent({ setup: () => () => null }))
  app.component('VTextField', defineComponent({
    props: ['type', 'modelValue'],
    setup: props => () => h('input', { type: props.type, value: props.modelValue }),
  }))
  app.provide(ssrContextKey, { modules: new Set() })
  const form = app.mount(root) as unknown as { init: (result: InitResult) => Promise<void> }
  return { app, form, root }
}
const visibleElements = (node: Element): Element[] => node.style.display === 'none' || node.tag === '#comment'
  ? [] : [node, ...node.children.flatMap(visibleElements)]

const data = { signaturePublicKey: 'trusted-key', authenticated: false, passwordRequired: false }
beforeEach(() => {
  vi.clearAllMocks()
  vi.stubGlobal('__IS_TAURI__', false)
  mock.identity.mockReturnValue(undefined)
  mock.token.mockReturnValue('stale-token')
})
afterEach(() => vi.unstubAllGlobals())

it('does not treat a missing password as acceptance of a stale stored token', () => {
  const success = vi.fn()
  const login = useLogin({ redirectOnSuccess: false, onSuccess: success })
  expect(login.applyInitResult({ status: 200, data })).toBe(false)
  expect(success).not.toHaveBeenCalled()
  expect(clearCurrentSession).toHaveBeenCalledTimes(1)
  expect(login.password.value).toBe('')
  expect(login.showPasswordInput.value).toBe(false)
})

it('auto-authenticates only after explicit session acceptance', () => {
  const success = vi.fn()
  const login = useLogin({ redirectOnSuccess: false, onSuccess: success })
  expect(login.applyInitResult({ status: 200, data: { ...data, authenticated: true } })).toBe(true)
  expect(success).toHaveBeenCalledTimes(1)
  expect(clearCurrentSession).not.toHaveBeenCalled()
})

it('shows the password input from the explicit fixed-password flag', () => {
  const login = useLogin({ redirectOnSuccess: false })
  login.password.value = 'old-autofill'
  expect(login.applyInitResult({ status: 200, data: { ...data, passwordRequired: true } })).toBe(false)
  expect(login.showPasswordInput.value).toBe(true)
  expect(login.password.value).toBe('')
})

it('rejects changed identities before auto-login', () => {
  const success = vi.fn()
  mock.identity.mockReturnValue({ deviceId: 'device', publicKey: 'previous-key' })
  const login = useLogin({ redirectOnSuccess: false, onSuccess: success })
  expect(login.applyInitResult({ status: 200, data: { ...data, authenticated: true } })).toBe(false)
  expect(clearCurrentSession).not.toHaveBeenCalled()
  expect(login.error.value).toBe('login.peer_identity_changed')
  expect(login.webAccessDisabled.value).toBe(true)
  expect(success).not.toHaveBeenCalled()
})

it('starts B management through Core and keeps the Web session on A', async () => {
  const state = ref<PairingView | null>(null)
  const start = vi.fn(async () => {
    state.value = { deviceId: 'device-b', generation: 'fixture', mode: 'MANAGE', phase: 'SUCCEEDED', error: null }
  })
  vi.mocked(createPairing).mockReturnValue({
    state, start, submit: vi.fn(), cancel: vi.fn(), dispose: vi.fn(),
  })
  const success = vi.fn()
  const login = useLogin({ redirectOnSuccess: false, onSuccess: success, managedDeviceId: 'device-b' })
  await login.startPairing()
  await nextTick()
  expect(start).toHaveBeenCalledWith({ peerId: 'device-b', mode: 'MANAGE', password: undefined })
  expect(success).toHaveBeenCalledOnce()
  expect(setRemoteClientId).not.toHaveBeenCalled()
  expect(saveBrowserSession).not.toHaveBeenCalled()
})

it('rejects an OPAQUE key change before auto-login', () => {
  const success = vi.fn()
  mock.identity.mockReturnValue({ deviceId: 'device', publicKey: data.signaturePublicKey, opaquePublicKey: 'old-key' })
  const login = useLogin({ redirectOnSuccess: false, onSuccess: success })
  login.applyInitResult({ status: 200, data: { ...data, passwordRequired: true, authenticated: true, opaquePublicKey: 'new-key' } })
  expect(login.error.value).toBe('login.peer_identity_changed')
  expect(login.webAccessDisabled.value).toBe(true)
  expect(success).not.toHaveBeenCalled()
})

it('passes the entered password only to the handshake and stores the authenticated pin after success', async () => {
  const login = useLogin({ redirectOnSuccess: false })
  login.applyInitResult({ status: 200, data: { ...data, passwordRequired: true, opaquePublicKey: 'init-key' } })
  expect(saveBrowserIdentity).not.toHaveBeenCalled()
  login.password.value = 'synthetic password'
  vi.mocked(performLoginHandshake).mockResolvedValueOnce({ deviceId: 'device', token: 'token', signaturePublicKey: data.signaturePublicKey, opaquePublicKey: 'init-key', chatPaired: false })
  await login.onSubmit()
  expect(performLoginHandshake).toHaveBeenCalledWith(expect.objectContaining({ password: 'synthetic password', initOpaquePublicKey: 'init-key' }))
  expect(vi.mocked(performLoginHandshake).mock.calls[0][0]).not.toHaveProperty('passwordHash')
  expect(login.password.value).toBe('')
  expect(saveBrowserIdentity).toHaveBeenCalledWith('device', data.signaturePublicKey, 'init-key')
})

const previousKey = btoa(String.fromCharCode(...new Uint8Array(32).fill(1)))
const nextKey = btoa(String.fromCharCode(...new Uint8Array(32).fill(2)))

it('keeps the old identity until explicitly trusted replacement authenticates', async () => {
  const previous = { deviceId: 'previous-device', publicKey: previousKey }
  mock.identity.mockReturnValue(previous)
  const login = useLogin({ redirectOnSuccess: false })
  login.applyInitResult({ status: 200, data: { ...data, signaturePublicKey: nextKey } })
  expect(login.webAccessDisabled.value).toBe(true)
  await login.confirmIdentityChange()
  expect(login.webAccessDisabled.value).toBe(false)
  expect(forgetBrowserIdentity).not.toHaveBeenCalled()
  expect(replaceBrowserIdentity).not.toHaveBeenCalled()
  vi.mocked(performLoginHandshake).mockResolvedValueOnce({ deviceId: 'new-device', token: 'token', signaturePublicKey: nextKey, chatPaired: false })
  await login.onSubmit()
  expect(performLoginHandshake).toHaveBeenCalledWith(expect.objectContaining({ storedSignaturePublicKey: nextKey, expectedDeviceId: undefined }))
  expect(replaceBrowserIdentity).toHaveBeenCalledWith(previous, { deviceId: 'new-device', publicKey: nextKey, opaquePublicKey: undefined })
  expect(saveBrowserIdentity).not.toHaveBeenCalled()
})

it('does not replace the saved identity when authentication fails', async () => {
  mock.identity.mockReturnValue({ deviceId: 'previous-device', publicKey: previousKey })
  const login = useLogin({ redirectOnSuccess: false })
  login.applyInitResult({ status: 200, data: { ...data, signaturePublicKey: nextKey } })
  await login.confirmIdentityChange()
  vi.mocked(performLoginHandshake).mockRejectedValueOnce(new Error('internal_error'))
  await login.onSubmit()
  expect(replaceBrowserIdentity).not.toHaveBeenCalled()
  expect(saveBrowserSession).not.toHaveBeenCalled()
  expect(login.error.value).toBe('login.internal_error')
})

it('requires new approval if the offered key changes again', async () => {
  mock.identity.mockReturnValue({ deviceId: 'previous-device', publicKey: previousKey })
  const login = useLogin({ redirectOnSuccess: false })
  login.applyInitResult({ status: 200, data: { ...data, signaturePublicKey: nextKey } })
  await login.confirmIdentityChange()
  const thirdKey = btoa(String.fromCharCode(...new Uint8Array(32).fill(3)))
  login.applyInitResult({ status: 200, data: { ...data, signaturePublicKey: thirdKey } })
  expect(login.webAccessDisabled.value).toBe(true)
  expect(login.error.value).toBe('login.peer_identity_changed')
  expect(replaceBrowserIdentity).not.toHaveBeenCalled()
})

it.each([false, true])('confirms a signature change before showing the login form, passwordRequired=%s', async passwordRequired => {
  mock.identity.mockReturnValue({ deviceId: 'previous-device', publicKey: previousKey })
  const { app, form, root } = mountLoginForm()
  try {
    await form.init({ status: 200, data: { ...data, signaturePublicKey: nextKey, passwordRequired } })
    await nextTick()
    let visible = visibleElements(root)
    expect(visible.some(node => node.tag === 'form' || node.tag === 'input' || node.tag === 'code')).toBe(false)
    expect(visible.filter(node => node.props.role === 'alert')).toHaveLength(1)
    expect(visible.map(node => node.text).join('')).toContain('login.peer_identity_changed')
    const buttons = visible.filter(node => node.tag === 'button')
    expect(buttons).toHaveLength(1)
    expect(visibleElements(buttons[0]).map(node => node.text).join('')).toContain('login.confirm_identity_change')
    await (buttons[0].props.onClick as () => Promise<void>)()
    await nextTick()
    visible = visibleElements(root)
    expect(visible.filter(node => node.tag === 'form')).toHaveLength(1)
    expect(visible.filter(node => node.tag === 'input' && node.props.type === 'password')).toHaveLength(passwordRequired ? 1 : 0)
    expect(visible.map(node => node.text).join('')).toContain('log_in')
    expect(performLoginHandshake).not.toHaveBeenCalled()
    expect(replaceBrowserIdentity).not.toHaveBeenCalled()
    expect(forgetBrowserIdentity).not.toHaveBeenCalled()
  } finally { app.unmount() }
})

it('confirms an OPAQUE key change before authenticating with the new key', async () => {
  const previousOpaqueKey = btoa(String.fromCharCode(...new Uint8Array(32).fill(3)))
  const nextOpaqueKey = btoa(String.fromCharCode(...new Uint8Array(32).fill(4)))
  const previous = { deviceId: 'device', publicKey: previousKey, opaquePublicKey: previousOpaqueKey }
  mock.identity.mockReturnValue(previous)
  const login = useLogin({ redirectOnSuccess: false })
  login.applyInitResult({ status: 200, data: { ...data, signaturePublicKey: previousKey, passwordRequired: true, opaquePublicKey: nextOpaqueKey } })
  expect(login.webAccessDisabled.value).toBe(true)
  await login.confirmIdentityChange()
  expect(replaceBrowserIdentity).not.toHaveBeenCalled()
  login.password.value = 'synthetic password'
  vi.mocked(performLoginHandshake).mockResolvedValueOnce({ deviceId: 'device', token: 'token', signaturePublicKey: previousKey, opaquePublicKey: nextOpaqueKey, chatPaired: false })
  await login.onSubmit()
  expect(performLoginHandshake).toHaveBeenCalledWith(expect.objectContaining({ password: 'synthetic password', storedOpaquePublicKey: nextOpaqueKey }))
  expect(replaceBrowserIdentity).toHaveBeenCalledWith(previous, { deviceId: 'device', publicKey: previousKey, opaquePublicKey: nextOpaqueKey })
})

it('allows only same-origin login redirects after URL normalization', () => {
  vi.stubGlobal('window', { location: { origin: 'https://device.example' } })
  expect(getSafeRedirect('/files?path=notes#top')).toBe('/files?path=notes#top')
  for (const value of ['//other.example', '/\\other.example', '/\n/other.example', 'https://other.example', 'javascript:alert(1)']) {
    expect(getSafeRedirect(value)).toBe('/')
  }
})
