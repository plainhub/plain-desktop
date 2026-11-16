import { afterEach, expect, it, vi } from 'vitest'
import { h, ref } from 'vue'
import { clientTemplate, renderView, nodes, click, flush, content } from '../helpers/vue-node-renderer'

const state = vi.hoisted(() => ({ modal: vi.fn(), fetch: vi.fn(), refetch: vi.fn(), view: vi.fn(), toast: vi.fn() }))
vi.mock('@/plugins/router', () => ({ default: { currentRoute: ref({ path: '/images', query: {} }) } }))
vi.mock('@/stores/main', () => ({ useMainStore: () => ({ bucketFilterCollapsed: {} }) }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('@/hooks/media', () => ({ useBuckets: () => ({ view: state.view }) }))
vi.mock('@/lib/api/query', () => ({
  mediaBucketsGQL: 'mediaBuckets',
  initQuery: ({ handle }: any) => {
    handle({ mediaBuckets: [{ id: '/fixture/Photos', name: 'Photos', itemCount: 12, topItemPaths: [] }] }, '')
    return { refetch: state.refetch }
  },
}))
vi.mock('@/lib/api/gql-client', () => ({ gqlFetch: state.fetch, GqlError: class extends Error {} }))
vi.mock('@/components/toaster', () => ({ default: state.toast }))
vi.mock('@/components/modal', () => ({ openModal: state.modal }))
vi.mock('@/components/EditValueModal.vue', () => ({ default: {} }))
vi.mock('@/components/BucketThumb.vue', () => ({ default: { render: () => null } }))
import BucketFilter from '@/components/BucketFilter.vue'
import SidebarListItem from '@/components/SidebarListItem.vue'
clientTemplate(BucketFilter, 'src/components/BucketFilter.vue')
clientTemplate(SidebarListItem, 'src/components/SidebarListItem.vue')

let unmount: (() => void) | undefined
afterEach(() => { unmount?.(); vi.clearAllMocks(); vi.unstubAllGlobals() })

async function openRename(type = 'IMAGE', selected = '/fixture/Photos') {
  vi.stubGlobal('document', { getElementById: vi.fn(), dispatchEvent: vi.fn() })
  vi.stubGlobal('CustomEvent', class { constructor(public type: string, public detail: unknown) {} })
  const view = renderView(BucketFilter, { type, selected }, {
    SidebarListItem,
    VIconButton: { setup(_: unknown, { slots, attrs }: any) { return () => h('button', attrs, slots.default?.()) } },
    VDropdownMenu: { props: ['modelValue', 'anchor'], setup(props: any, { slots }: any) { return () => props.modelValue ? h('menu', slots.default?.()) : null } },
  })
  unmount = view.unmount
  expect(content(view.root)).toContain('Photos12')
  const button = nodes(view.root).find(node => node.props['aria-label'] === 'actions: Photos')
  expect(button, 'folder actions entry').toBeDefined()
  click(button!)
  await flush()
  expect(state.view).not.toHaveBeenCalled()
  click(nodes(view.root).find(node => node.props.class === 'dropdown-item')!)
  await flush()
  expect(nodes(view.root).filter(node => node.tag === 'menu')).toHaveLength(0)
  return state.modal.mock.calls[0][1]
}

it.each(['IMAGE', 'VIDEO', 'AUDIO', 'DOC'])('renames a real %s folder and follows the returned path only after success', async type => {
  const options = await openRename(type)
  expect(options).toMatchObject({ value: 'Photos', borderless: true })
  expect(options.getVariables('Album')).toEqual({ path: '/fixture/Photos', name: 'Album' })
  expect(options.validate('../Album')).toBe('invalid_file_path')
  expect(options.validate('..')).toBe('invalid_file_path')
  expect(options.validate('Album')).toBe('')
  const mutation = options.mutation()
  state.fetch.mockResolvedValueOnce({ data: { renameFile: '/fixture/Album' } })
  await mutation.mutate(options.getVariables('Album'))
  expect(state.fetch).toHaveBeenCalledWith(expect.stringContaining('renameFile'), { path: '/fixture/Photos', name: 'Album' }, { dedupe: false })
  expect(options.done('Album')).toBe(true)
  expect(state.refetch).toHaveBeenCalledTimes(1)
  expect(state.view).toHaveBeenCalledWith(expect.anything(), '/fixture/Album')
  expect(state.toast).not.toHaveBeenCalled()
})

it('keeps the current filter when renaming another folder', async () => {
  const options = await openRename('AUDIO', '/fixture/Other')
  const mutation = options.mutation()
  state.fetch.mockResolvedValueOnce({ data: { renameFile: '/fixture/Album' } })
  await mutation.mutate(options.getVariables('Album'))
  expect(options.done('Album')).toBe(true)
  expect(state.refetch).toHaveBeenCalledTimes(1)
  expect(state.view).not.toHaveBeenCalled()
})

it('leaves the modal and filter unchanged for a null result and permits a successful retry', async () => {
  const options = await openRename()
  const mutation = options.mutation()
  state.fetch.mockResolvedValueOnce({ data: { renameFile: null } })
  await mutation.mutate(options.getVariables('Taken'))
  expect(options.done('Taken')).toBe(false)
  expect(state.toast).toHaveBeenCalledWith('rename_failed', 'error')
  expect(state.refetch).not.toHaveBeenCalled()
  expect(state.view).not.toHaveBeenCalled()
  state.fetch.mockResolvedValueOnce({ data: { renameFile: '/fixture/Album' } })
  await mutation.mutate(options.getVariables('Album'))
  expect(options.done('Album')).toBe(true)
  expect(state.refetch).toHaveBeenCalledTimes(1)
})

it('does not report success or refresh after a rejected mutation', async () => {
  const options = await openRename()
  const mutation = options.mutation()
  const done = vi.fn()
  mutation.onDone(done)
  state.fetch.mockResolvedValueOnce({ errors: [{ message: 'Write refused' }] })
  await mutation.mutate(options.getVariables('Album'))
  expect(done).not.toHaveBeenCalled()
  expect(state.refetch).not.toHaveBeenCalled()
  expect(state.view).not.toHaveBeenCalled()
})
