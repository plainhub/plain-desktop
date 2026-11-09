import { afterEach, expect, it, vi } from 'vitest'
import { createRenderer, ref, ssrContextKey } from 'vue'

const fixture = vi.hoisted(() => ({ source: undefined as any, owner: undefined as any, invoke: vi.fn(), close: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke: fixture.invoke }))
vi.mock('@/lib/api/native-client', () => ({ nativeViewScope: async () => 'document' }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => false }))
vi.mock('@/lib/device/client-id', () => ({ getRemoteClientId: () => 'peer' }))
vi.mock('@/lib/device/login-peers', () => ({ getLoginPeerRevision: () => 0 }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('@/components/toaster', () => ({ default: vi.fn() }))
vi.mock('@/lib/api/file', () => ({ getFileName: (path: string) => path }))
vi.mock('@/hooks/lightbox', async () => {
  const { ref } = await import('vue')
  return {
    useLightboxState: () => ({ current: ref(fixture.source), tempStore: { lightbox: { readOnly: false } }, imageViewQuality: ref('original') }),
    useLightboxQueries: () => ({}), useLightboxTransform: () => ({}),
    useLightboxNavigation: () => ({ closeDialog: fixture.close }),
    useLightboxFileActions: () => ({}), useLightboxEvents: () => ({}), useLightboxMouseTouch: () => ({}),
    getImageDisplayUrl: vi.fn(),
  }
})
vi.mock('@/hooks/organize-undo', () => ({ useOrganizeUndo: () => ({ count: ref(0) }) }))
vi.mock('@/components/modal', () => ({ promptModal: vi.fn() }))
vi.mock('@/components/lightbox/LightboxMoveToFolders.vue', () => ({ default: {} }))
vi.mock('@/components/lightbox/OrganizeConfirmModal.vue', () => ({ default: {} }))
import { createNativeResourceOwner } from '@/hooks/native-resource-url'
import Lightbox from '@/components/lightbox/lightbox.vue'
Lightbox.render = () => null
const renderer = createRenderer<object, object>({
  insert: vi.fn(), remove: vi.fn(), createElement: () => ({}), createText: () => ({}), createComment: () => ({}),
  setText: vi.fn(), setElementText: vi.fn(), parentNode: () => null, nextSibling: () => null, patchProp: vi.fn(),
})
let unmount: (() => void) | undefined
afterEach(() => { unmount?.(); vi.unstubAllGlobals() })

it('captures the native peer reference for a popup before releasing the lightbox owner', async () => {
  vi.stubGlobal('__IS_TAURI__', true)
  vi.stubGlobal('window', { addEventListener: vi.fn(), removeEventListener: vi.fn() })
  fixture.invoke.mockImplementation(async (command: string) => command === 'native_resource_open'
    ? { id: 'shared', url: 'plain-resource://file/shared', size: 4, name: 'image.png', mime: 'image/png', inlineAllowed: true } : true)
  fixture.owner = createNativeResourceOwner()
  fixture.source = { src: await fixture.owner.url('/peer/image.png'), path: '/peer/image.png', name: 'image.png', size: 4, durationMs: 0 }
  fixture.close.mockImplementation(() => { void fixture.owner.dispose(); fixture.source.src = '' })
  const app = renderer.createApp(Lightbox)
  app.provide('isPhone', false); app.provide('isTablet', false); app.provide(ssrContextKey, { modules: new Set() })
  const component = app.mount({}) as any; unmount = () => app.unmount()
  await component.$.setupState.onOpenInWindow()
  const calls = fixture.invoke.mock.calls
  const activate = calls.findIndex(([command]) => command === 'media_preview_activate')
  const close = calls.findIndex(([command]) => command === 'native_resource_close')
  expect(activate).toBeGreaterThan(-1)
  expect(close).toBeGreaterThan(activate)
  expect(calls[activate][1].source).toMatchObject({ path: '/peer/image.png', src: '', __cid: 'peer' })
})
