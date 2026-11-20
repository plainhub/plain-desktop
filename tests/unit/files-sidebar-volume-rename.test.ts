import { afterEach, expect, it, vi } from 'vitest'
import { h, ref } from 'vue'
import { clientTemplate, renderView, nodes, fire, flush } from '../helpers/vue-node-renderer'

const state = vi.hoisted(() => ({ mounts: null as any, modal: vi.fn(), fetch: vi.fn(), refetch: vi.fn(), navigate: vi.fn(), toast: vi.fn() }))
vi.mock('@/plugins/router', () => ({ default: { currentRoute: ref({ path: '/files', fullPath: '/files', query: {} }) }, replacePath: state.navigate }))
vi.mock('@/stores/main', () => ({ useMainStore: () => ({}) }))
vi.mock('@/stores/temp', () => ({ useTempStore: () => ({ app: ref({ capabilities: [] }) }) }))
vi.mock('pinia', () => ({ storeToRefs: (store: any) => store }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('@/hooks/files', () => ({ useMounts: () => ({ mounts: state.mounts, refetch: state.refetch }), useFavoriteFolders: () => ({ favoriteFolders: ref([]) }), useSearch: () => ({ parseQ: vi.fn() }) }))
vi.mock('@/lib/api/gql-client', () => ({ gqlFetch: state.fetch, GqlError: class extends Error {} }))
vi.mock('@/components/toaster', () => ({ default: state.toast }))
vi.mock('@/plugins/eventbus', () => ({ default: { emit: vi.fn() } }))
vi.mock('@/components/modal', () => ({ openModal: state.modal }))
vi.mock('@/components/EditValueModal.vue', () => ({ default: {} }))
vi.mock('@/components/storage/DiskManagerModal.vue', () => ({ default: {} }))
vi.mock('@/components/storage/VolumeCard.vue', () => ({ default: { setup(_: unknown, { slots }: any) { return () => h('article', slots.actions?.()) } } }))
import FilesSidebar from '@/views/files/FilesSidebar.vue'
clientTemplate(FilesSidebar, 'src/views/files/FilesSidebar.vue')

let unmount: (() => void) | undefined
afterEach(() => { unmount?.(); vi.clearAllMocks(); vi.unstubAllGlobals() })

it('opens volume rename from its menu, sends the stable volume id and refreshes only after success', async () => {
  state.mounts = ref([{ id: 'volume-fixture', mountPoint: '/media/photos', name: 'Photos', alias: 'Archive', driveType: 'USB_STORAGE', totalBytes: 0 }])
  vi.stubGlobal('document', { getElementById: vi.fn(), dispatchEvent: vi.fn() })
  vi.stubGlobal('CustomEvent', class { constructor(public type: string, public detail: unknown) {} })
  const view = renderView(FilesSidebar, {}, {
    LeftSidebar: { setup(_: unknown, { slots }: any) { return () => h('aside', slots.body?.()) } },
    SidebarListItem: { render: () => null },
    VIconButton: { setup(_: unknown, { slots, attrs }: any) { return () => h('button', attrs, slots.default?.()) } },
    VDropdownMenu: { props: ['modelValue', 'anchor'], setup(props: any, { slots }: any) { return () => props.modelValue ? h('menu', slots.default?.()) : null } },
  })
  unmount = view.unmount
  const button = nodes(view.root).find(node => node.props.id === 'volume-/media/photos')!
  expect(button.props['aria-label']).toBe('actions: Archive')
  expect(button.props['aria-expanded']).toBe(false)
  fire(button, 'onClick', { stopPropagation() {}, preventDefault() {} })
  await flush()
  expect(button.props['aria-expanded']).toBe(true)
  expect(state.navigate).not.toHaveBeenCalled()
  const rename = nodes(view.root).find(node => node.props.class === 'dropdown-item')!
  fire(rename, 'onClick', {})
  await flush()
  expect(nodes(view.root).filter(node => node.tag === 'menu')).toHaveLength(0)
  expect(button.props['aria-expanded']).toBe(false)
  const options = state.modal.mock.calls[0][1]
  expect(options).toMatchObject({ value: 'Archive', allowEmpty: true, borderless: true })
  expect(options.done).toBeUndefined()
  expect(options.getVariables(' Photos ')).toEqual({ id: 'volume-fixture', alias: 'Photos' })
  expect(options.getVariables('  ')).toEqual({ id: 'volume-fixture', alias: '' })
  const mutation = options.mutation()
  state.fetch.mockResolvedValueOnce({ errors: [{ message: 'Write refused' }] })
  await mutation.mutate(options.getVariables('New name'))
  expect(state.refetch).not.toHaveBeenCalled()
  expect((await import('@/plugins/eventbus')).default.emit).toHaveBeenCalledExactlyOnceWith('toast', 'Write refused')
  state.fetch.mockResolvedValueOnce({ data: { setMountAlias: true } })
  await mutation.mutate(options.getVariables('New name'))
  expect(state.fetch).toHaveBeenLastCalledWith(expect.stringContaining('setMountAlias'), { id: 'volume-fixture', alias: 'New name' }, { dedupe: false })
  expect(state.refetch).toHaveBeenCalledTimes(1)
  state.fetch.mockResolvedValueOnce({ data: { setMountAlias: true } })
  await mutation.mutate(options.getVariables(''))
  expect(state.fetch).toHaveBeenLastCalledWith(expect.any(String), { id: 'volume-fixture', alias: '' }, { dedupe: false })
})
