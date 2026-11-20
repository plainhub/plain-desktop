import { expect, it, vi } from 'vitest'
import { ref } from 'vue'
import RailSettingsPopup from '@/views/app-rail/RailSettingsPopup.vue'
import NearbyView from '@/views/chat/NearbyView.vue'
import { click, clientTemplate, nodes, renderView } from '../helpers/vue-node-renderer'

const { pushModal } = vi.hoisted(() => ({ pushModal: vi.fn() }))
const stub = () => null
vi.mock('@/components/modal', () => ({ pushModal, openModal: vi.fn() }))
vi.mock('@/views/chat/NearbyView.vue', () => ({ default: { render: () => null } }))
vi.mock('@/components/ThemeChanger.vue', () => ({ default: { render: () => null } }))
vi.mock('@/components/EditValueModal.vue', () => ({ default: { render: () => null } }))
vi.mock('@/stores/temp', () => ({ useTempStore: () => ({ app: ref({}) }) }))
vi.mock('@/stores/main', () => ({ useMainStore: () => ({ railFeatures: [] }) }))
vi.mock('pinia', () => ({ storeToRefs: (store: unknown) => store }))
vi.mock('vue-router', () => ({ useRouter: vi.fn() }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('@/lib/device/login-peers', () => ({ findLoginPeer: vi.fn() }))
vi.mock('@/lib/device/client-id', () => ({ getRemoteClientId: vi.fn() }))
vi.mock('@/views/app-rail/features', () => ({ getAvailableFeatures: () => [] }))
vi.mock('@/lib/api/tauri-window', () => ({ openAboutWindow: vi.fn() }))
vi.mock('@/hooks/show-action-error', () => ({ showActionError: vi.fn() }))
vi.mock('@/composables/useBrowserLogout', () => ({ useBrowserLogout: vi.fn() }))
vi.mock('@/composables/useLocaleSwitch', () => ({ useLocaleSwitch: () => ({ locale: ref('en'), availableLocales: ref([]) }) }))
vi.mock('@/lib/api/mutation', () => ({ initMutation: vi.fn(), updateDeviceNameGQL: 'rename' }))
vi.mock('@/lib/api/query', () => ({ initQuery: () => ({ refetch: vi.fn() }), deviceStatusGQL: 'status' }))

it('opens Nearby Devices from the settings popup on an HTTP web origin', () => {
  vi.stubGlobal('__IS_TAURI__', false)
  vi.stubGlobal('window', { location: new URL('http://plain.test:8080') })
  try {
    const view = renderView(clientTemplate(RailSettingsPopup, 'src/views/app-rail/RailSettingsPopup.vue'), {}, {
      'v-dropdown': { inheritAttrs: false, setup: (_: unknown, { slots }: { slots: { default: () => unknown } }) => slots.default },
      'v-icon-button': stub,
      'router-link': stub,
    })
    try {
      const button = nodes(view.root).find(node => node.props['aria-label'] === 'device_discovery.change_device')
      expect(button).toBeDefined()
      click(button!)
      expect(pushModal).toHaveBeenCalledExactlyOnceWith(NearbyView, { origin: true })
    } finally { view.unmount() }
  } finally { vi.unstubAllGlobals() }
})
