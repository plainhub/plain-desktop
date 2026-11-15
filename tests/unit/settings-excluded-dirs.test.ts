import { afterEach, expect, it, vi } from 'vitest'
import { h, reactive } from 'vue'
import View from '@/views/settings/SettingsExcludedDirsView.vue'
import { click, clientTemplate, content, flush, nodes, renderView } from '../helpers/vue-node-renderer'

const mock = vi.hoisted(() => ({ values: null as any, set: vi.fn(), preload: vi.fn(), picker: vi.fn() }))
vi.mock('@/lib/prefs/core', () => ({
  preload: () => mock.preload(),
  get: (key: string, fallback: unknown) => mock.values[key] ?? fallback,
  set: (...args: unknown[]) => mock.set(...args),
}))
vi.mock('@/components/modal', () => ({ promptModal: (...args: unknown[]) => mock.picker(...args) }))
vi.mock('@/components/DirectoryPickerModal.vue', () => ({ default: { render: () => null } }))
const Button = { props: ['disabled'], setup: (props: any, { slots }: any) => () => h('button', { disabled: props.disabled }, slots.default?.()) }
const mounted: (() => void)[] = []
afterEach(() => { mounted.splice(0).forEach(stop => stop()); vi.clearAllMocks() })
function mount(paths: string[] = [], loadError?: Error) {
  mock.preload.mockResolvedValue(undefined)
  if (loadError) mock.preload.mockRejectedValueOnce(loadError)
  mock.values = reactive({ media_excluded_dirs: paths })
  mock.set.mockImplementation(async (key: string, paths: string[]) => { mock.values[key] = paths })
  const view = renderView(clientTemplate(View, 'src/views/settings/SettingsExcludedDirsView.vue'), {}, { VIconButton: Button, VTextButton: Button })
  mounted.push(view.unmount)
  return view.root
}

it('adds and removes device media exclusions through preferences without index operations', async () => {
  const root = mount(['/hidden'])
  await flush()
  expect(content(root)).toContain('/hidden')
  mock.picker.mockResolvedValueOnce('/another')
  click(nodes(root).find(node => node.tag === 'button' && content(node) === 'file_index.add_directory')!)
  await flush()
  expect(mock.set).toHaveBeenLastCalledWith('media_excluded_dirs', ['/hidden', '/another'])
  expect(content(root)).toContain('/another')
  click(nodes(root).find(node => node.tag === 'button' && node.props['aria-label'] === 'remove')!)
  await flush()
  expect(mock.set).toHaveBeenLastCalledWith('media_excluded_dirs', ['/another'])
})

it('keeps saved directories visible on failure and shows the error', async () => {
  const root = mount(['/hidden'])
  await flush()
  mock.set.mockRejectedValueOnce(new Error('Write refused'))
  click(nodes(root).find(node => node.tag === 'button' && node.props['aria-label'] === 'remove')!)
  await flush()
  expect(content(root)).toContain('/hidden')
  expect(nodes(root).find(node => node.props.role === 'alert')?.text).toBe('Write refused')
})

it('ignores a cancelled picker and duplicate paths and reflects device preference updates', async () => {
  const root = mount(['/hidden'])
  await flush()
  const add = () => click(nodes(root).find(node => node.tag === 'button' && content(node) === 'file_index.add_directory')!)
  mock.picker.mockResolvedValueOnce(undefined).mockResolvedValueOnce('/hidden')
  add(); await flush(); add(); await flush()
  expect(mock.set).not.toHaveBeenCalled()
  mock.values.media_excluded_dirs = ['/updated']
  await flush()
  expect(content(root)).toContain('/updated')
  expect(content(root)).not.toContain('/hidden')
})

it('loads saved preferences before enabling changes and allows retry after a read failure', async () => {
  const root = mount(['/hidden'], new Error('Read refused'))
  expect(nodes(root).filter(node => node.tag === 'button')).toHaveLength(0)
  await flush()
  expect(content(root)).toContain('Read refused')
  expect(content(root)).not.toContain('/hidden')
  click(nodes(root).find(node => node.tag === 'button' && content(node) === 'retry')!)
  await flush()
  expect(content(root)).toContain('/hidden')
  expect(mock.preload).toHaveBeenCalledTimes(2)
  expect(mock.set).not.toHaveBeenCalled()
})
