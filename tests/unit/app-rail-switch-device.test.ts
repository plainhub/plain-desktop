import { afterEach, expect, it, vi } from 'vitest'
import { h, ref } from 'vue'
import RailSettingsPopup from '@/views/app-rail/RailSettingsPopup.vue'
import NearbyView from '@/views/chat/NearbyView.vue'
import { click, clientTemplate, nodes, renderView } from '../helpers/vue-node-renderer'

const mocks = vi.hoisted(() => ({ pushModal: vi.fn() }))
vi.mock('@/stores/temp', () => ({ useTempStore: () => ({ app: ref({ deviceName: 'Current device' }) }) }))
vi.mock('@/stores/main', () => ({ useMainStore: () => ({ railFeatures: [], lastRoutes: {} }) }))
vi.mock('pinia', () => ({ storeToRefs: (store: unknown) => store }))
vi.mock('vue-router', () => ({ useRouter: () => ({}) }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('@/lib/device/login-peers', () => ({ findLoginPeer: () => undefined }))
vi.mock('@/lib/device/client-id', () => ({ getRemoteClientId: () => 'current' }))
vi.mock('@/components/modal', () => ({ pushModal: mocks.pushModal, openModal: vi.fn() }))
vi.mock('@/views/app-rail/features', () => ({ getAvailableFeatures: () => [] }))
vi.mock('@/lib/platform', () => ({ isMacPlatform: () => false }))
vi.mock('@/lib/api/tauri-window', () => ({ openAboutWindow: vi.fn() }))
vi.mock('@/composables/useBrowserLogout', () => ({ useBrowserLogout: () => vi.fn() }))
vi.mock('@/composables/useLocaleSwitch', () => ({ useLocaleSwitch: () => ({ locale: ref('en-US'), availableLocales: [] }) }))
vi.mock('@/components/ThemeChanger.vue', () => ({ default: { render: () => null } }))
vi.mock('@/views/chat/NearbyView.vue', () => ({ default: { render: () => null } }))
vi.mock('@/components/EditValueModal.vue', () => ({ default: { render: () => null } }))
vi.mock('@/lib/api/query', () => ({ deviceStatusGQL: 'deviceStatus', initQuery: () => ({ refetch: vi.fn() }) }))
vi.mock('@/lib/api/mutation', () => ({ initMutation: vi.fn() }))

afterEach(() => { vi.unstubAllGlobals(); mocks.pushModal.mockClear() })

it.each([false, true])('opens the correct device picker from the App Rail (Tauri: %s)', (tauri) => {
  vi.stubGlobal('__IS_TAURI__', tauri)
  const view = renderView(clientTemplate(RailSettingsPopup, 'src/views/app-rail/RailSettingsPopup.vue'), {}, {
    VDropdown: { setup: (_, { slots }) => () => h('div', slots.default?.()) },
    VIconButton: { setup: (_, { slots }) => () => h('button', slots.default?.()) },
    RouterLink: { setup: (_, { slots }) => () => h('a', slots.default?.()) },
  })
  try {
    const button = nodes(view.root).find(node => node.props['aria-label'] === 'device_discovery.change_device')
    expect(button).toBeDefined()
    click(button!)
    const args = tauri ? [NearbyView] : [NearbyView, { switchOnly: true }]
    expect(mocks.pushModal).toHaveBeenCalledExactlyOnceWith(...args)
  } finally { view.unmount() }
})
