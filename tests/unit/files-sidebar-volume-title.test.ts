import { expect, it, vi } from 'vitest'
import { effectScope, ref } from 'vue'
import type { IStorageMount } from '@/lib/interfaces'
const state = vi.hoisted(() => ({ mounts: null as any, favorites: null as any }))
vi.mock('@/lib/prefs/appearance', () => ({ get: (_key: string, fallback: unknown) => fallback }))
vi.mock('@/plugins/router', () => ({ default: { currentRoute: ref({ path: '/files', fullPath: '/files', query: {} }) }, replacePath: vi.fn() }))
vi.mock('@/stores/main', () => ({ useMainStore: () => ({}) }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('@/hooks/files', () => ({ useMounts: () => ({ mounts: state.mounts }), useFavoriteFolders: () => ({ favoriteFolders: state.favorites }), useSearch: () => ({ parseQ: vi.fn() }) }))
vi.mock('@/lib/api/mutation', () => ({ initMutation: () => ({ mutate: vi.fn(), onDone: vi.fn() }), removeFavoriteFolderGQL: '', setFavoriteFolderAliasGQL: '' }))
vi.mock('@/components/toaster', () => ({ default: vi.fn() }))
vi.mock('@/plugins/eventbus', () => ({ default: { emit: vi.fn() } }))
vi.mock('@/components/modal', () => ({ openModal: vi.fn() }))
vi.mock('@/components/EditValueModal.vue', () => ({ default: {} }))
import { useFilesSidebar } from '@/hooks/files-sidebar'

it('uses live volume names in sidebar links and favorite paths instead of USB numbering', () => {
  state.mounts = ref([{ mountPoint: '/media/photos', name: 'usb1', driveType: 'USB_STORAGE', diskModel: 'Example Drive', totalBytes: 314e9 }] as IStorageMount[])
  state.favorites = ref([{ rootPath: '/media/photos', fullPath: '/media/photos/Trips', alias: '' }])
  const scope = effectScope()
  const sidebar = scope.run(() => useFilesSidebar())!
  try {
    expect(sidebar.volumeLinks.value[0].title).toBe('Example Drive (314.0 GB)')
    expect(sidebar.favoriteLinks.value[0].title).toBe('Example Drive (314.0 GB)/Trips')
    state.mounts.value[0].label = 'Photos'
    expect(sidebar.volumeLinks.value[0].title).toBe('Photos')
    state.mounts.value[0].alias = 'Archive'
    expect(sidebar.volumeLinks.value[0].title).toBe('Archive')
    expect(sidebar.favoriteLinks.value[0].title).toBe('Archive/Trips')
  } finally { scope.stop() }
})
