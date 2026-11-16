import { expect, it, vi } from 'vitest'
import { h, nextTick, reactive, ref } from 'vue'
import AppRail from '@/views/app-rail/AppRail.vue'
import { clientTemplate, nodes, renderView } from '../helpers/vue-node-renderer'

const fixture = vi.hoisted(() => ({ store: {} as any, route: {} as any }))
vi.mock('@/stores/main', () => ({ useMainStore: () => fixture.store }))
vi.mock('@/stores/temp', () => ({ useTempStore: () => ({ app: ref({}) }) }))
vi.mock('pinia', () => ({ storeToRefs: (store: unknown) => store }))
vi.mock('vue-router', () => ({ useRouter: () => ({
  currentRoute: fixture.route, afterEach: () => vi.fn(),
  resolve: (path: string) => ({ meta: { group: path.split('/')[1].split('?')[0] } }),
}) }))
vi.mock('@/views/app-rail/RailSettingsPopup.vue', () => ({ default: { render: () => null } }))
vi.mock('@/views/app-rail/features', () => {
  const features = ['notes', 'files'].map(group => ({ id: group, group, defaultPath: `/${group}`, titleKey: group, icon: { render: () => null } }))
  return { ALL_FEATURES: features, getAvailableFeatures: () => features }
})
it('records the current entry route immediately and keeps later page routes separate', async () => {
  fixture.store = reactive({ railFeatures: ['notes', 'files'], lastRoutes: { notes: '/notes?q=stale', files: '/files?q=directory' } })
  fixture.route = ref({ path: '/notes', fullPath: '/notes', meta: { group: 'notes' } })
  const view = renderView(clientTemplate(AppRail, 'src/views/app-rail/AppRail.vue'), {}, {
    RouterLink: { props: ['to'], setup: (props: { to: string }, { slots }: any) => () => h('a', { href: props.to }, slots.default?.()) },
  })
  try {
    expect(fixture.store.lastRoutes.notes).toBe('/notes')
    expect(nodes(view.root).find(node => node.props['aria-label'] === 'notes')?.props.href).toBe('/notes')
    fixture.route.value = { path: '/files', fullPath: '/files?q=directory', meta: { group: 'files' } }
    await nextTick()
    expect(fixture.store.lastRoutes.notes).toBe('/notes')
    expect(fixture.store.lastRoutes.files).toBe('/files?q=directory')
  } finally { view.unmount() }
  fixture.route.value = { path: '/notes', fullPath: '/notes?q=after-unmount', meta: { group: 'notes' } }
  await nextTick()
  expect(fixture.store.lastRoutes.notes).toBe('/notes')
})
