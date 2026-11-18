import { beforeEach, expect, it, vi } from 'vitest'
import { ref } from 'vue'
import ScopeDialog from '@/views/home/file-index/FileIndexScopeDialog.vue'
import VolumeItem from '@/views/home/file-index/FileIndexVolumeItem.vue'
import VolumeCard from '@/views/home/file-index/FileIndexVolumeCard.vue'
import VIconButton from '@/components/base/VIconButton.vue'
import VTextButton from '@/components/base/VTextButton.vue'
import VFilledButton from '@/components/base/VFilledButton.vue'
import VCircularProgress from '@/components/base/VCircularProgress.vue'
import type { FileIndexMode, FileIndexVolume } from '@/lib/api/file-index'
import { click, clientTemplate, content, fire, flush, ModalShell, nodes, renderView, type Node } from '../helpers/vue-node-renderer'

const mocks = vi.hoisted(() => ({ request: vi.fn(), pick: vi.fn(), close: vi.fn(), notify: vi.fn(), stop: vi.fn() }))
vi.mock('@/lib/api/gql-client', () => ({ bindGraphqlClient: () => mocks.request }))
vi.mock('@/components/modal', () => ({ promptModal: mocks.pick, closeById: mocks.close, modalQueue: { value: [{ id: 7 }] } }))
vi.mock('@/components/toaster', () => ({ default: mocks.notify }))
vi.mock('@/components/DirectoryPickerModal.vue', () => ({ default: {} }))
vi.mock('@/views/home/file-index/FileIndexDefaults.vue', () => ({ default: { render: () => null } }))
vi.mock('@/components/base/VDropdown.vue', () => ({ default: { render: () => null } }))
vi.mock('@/lib/format', () => ({ formatDateTime: String, formatFileSize: String }))
vi.mock('@/views/home/home', () => ({
  useScanAction: () => ({ fileIndexScanProgress: ref({ state: 'IDLE' }), loading: ref(false), stop: mocks.stop }),
}))

for (const [component, path] of [
  [ScopeDialog, 'views/home/file-index/FileIndexScopeDialog.vue'],
  [VolumeItem, 'views/home/file-index/FileIndexVolumeItem.vue'],
  [VolumeCard, 'views/home/file-index/FileIndexVolumeCard.vue'],
  [VIconButton, 'components/base/VIconButton.vue'],
  [VTextButton, 'components/base/VTextButton.vue'],
  [VFilledButton, 'components/base/VFilledButton.vue'],
  [VCircularProgress, 'components/base/VCircularProgress.vue'],
] as const) clientTemplate(component, `src/${path}`)

const components = { VModal: ModalShell, VTextButton, VFilledButton, VIconButton, VCircularProgress }
const volume = (indexMode: FileIndexMode): FileIndexVolume => ({
  id: 'fixture', path: '/fixture', isAvailable: true, indexMode, includedDirs: [],
  excludedDirs: ['/fixture/ignored'], lastScanAt: null, lastScanStatus: null, lastScanCount: null, lastScanTotal: null,
})

function scope(indexMode: FileIndexMode) {
  const onSaved = vi.fn()
  const v = volume(indexMode)
  const view = renderView(ScopeDialog, { modalId: 'modal-7', volume: v, volumeName: 'Fixture volume', defaults: {}, onSaved }, components)
  return { ...view, onSaved, volume: v }
}

function button(root: Node, label: string) {
  return nodes(root).find(n => n.tag === 'button' && (content(n) === label || n.props['aria-label'] === label))!
}

function radios(root: Node) {
  return nodes(root).filter(n => n.tag === 'input' && n.props.type === 'radio')
}

beforeEach(() => {
  vi.resetAllMocks()
  mocks.request.mockResolvedValue({ data: { setFileIndexVolume: true } })
})

it('adds excluded folders only to the copy and saves the existing mutation', async () => {
  const view = scope('UNCONFIGURED')
  try {
    expect(radios(view.root).map(r => [r.props.value, r.props.checked])).toEqual([['BLOCKLIST', true], ['ALLOWLIST', false]])
    expect(content(view.root)).toContain('Fixture volume')
    mocks.pick.mockResolvedValue('/fixture/private')
    click(button(view.root, 'file_index.exclude_folder'))
    await flush()
    expect(mocks.pick).toHaveBeenCalledWith(expect.anything(), { initialPath: '/fixture', volume: { mountPoint: '/fixture', name: 'Fixture volume' } })
    expect(view.volume.excludedDirs).toEqual(['/fixture/ignored'])
    click(button(view.root, 'save'))
    await flush()
    expect(mocks.request).toHaveBeenCalledWith(expect.stringContaining('setFileIndexVolume(input: $input)'), {
      input: { volumeId: 'fixture', indexMode: 'BLOCKLIST', includedDirs: [], excludedDirs: ['/fixture/ignored', '/fixture/private'] },
    })
    expect(view.onSaved).toHaveBeenCalledOnce()
    expect(mocks.close).toHaveBeenCalledWith(7)
  } finally { view.unmount() }
})

it('does not enable a disabled volume or save an empty selection; retains the draft on error', async () => {
  const view = scope('DISABLED')
  try {
    expect(radios(view.root).map(r => r.props.checked)).toEqual([false, false])
    expect(button(view.root, 'save').props.disabled).toBe(true)
    click(button(view.root, 'save'))
    expect(mocks.request).not.toHaveBeenCalled()
    fire(radios(view.root)[1]!, 'onChange', {})
    await flush()
    expect(button(view.root, 'save').props.disabled).toBe(true)
    mocks.pick.mockResolvedValue('/fixture/photos')
    click(button(view.root, 'file_index.select_folder'))
    await flush()
    expect(button(view.root, 'save').props.disabled).toBe(false)
    expect(content(view.root).match(/file_index\.includes_subfolders/g)).toHaveLength(1)
    mocks.request.mockResolvedValueOnce({ errors: [{ message: 'Could not save scope' }] })
    click(button(view.root, 'save'))
    await flush()
    expect(mocks.request).toHaveBeenCalledWith(expect.any(String), {
      input: { volumeId: 'fixture', indexMode: 'ALLOWLIST', includedDirs: ['/fixture/photos'], excludedDirs: ['/fixture/ignored'] },
    })
    expect(content(view.root)).toContain('Could not save scope')
    expect(mocks.close).not.toHaveBeenCalled()
    expect(view.onSaved).not.toHaveBeenCalled()
    click(button(view.root, 'remove: /fixture/photos'))
    await flush()
    expect(button(view.root, 'save').props.disabled).toBe(true)
    click(button(view.root, 'cancel'))
    await flush()
    expect(mocks.close).toHaveBeenCalledWith(7)
    expect(mocks.request).toHaveBeenCalledOnce()
  } finally { view.unmount() }
})

it('keeps Rebuild on the card and moves Turn off to the gear dialog', async () => {
  const view = renderView(VolumeItem, { volume: volume('ALLOWLIST'), title: 'Fixture volume' }, components)
  try {
    expect(button(view.root, 'rebuild_index')).toBeDefined()
    expect(button(view.root, 'file_index.disable')).toBeUndefined()
  } finally { view.unmount() }
  const settings = scope('ALLOWLIST')
  try {
    expect(radios(settings.root)).toHaveLength(2)
    expect(button(settings.root, 'file_index.disable')).toBeDefined()
  } finally { settings.unmount() }
})

it('turns off only the selected volume from settings without saving draft folder changes', async () => {
  const view = scope('BLOCKLIST')
  try {
    mocks.pick.mockResolvedValue('/fixture/unsaved')
    click(button(view.root, 'file_index.exclude_folder'))
    await flush()
    click(button(view.root, 'file_index.disable'))
    await flush()
    expect(mocks.request).toHaveBeenCalledExactlyOnceWith(expect.any(String), {
      input: { volumeId: 'fixture', indexMode: 'DISABLED', includedDirs: [], excludedDirs: ['/fixture/ignored'] },
    })
    expect(mocks.stop).not.toHaveBeenCalled()
    expect(view.onSaved).toHaveBeenCalledOnce()
    expect(mocks.close).toHaveBeenCalledWith(7)
  } finally { view.unmount() }
})

it.each([
  ['RUNNING', 'file_index.scanning', 'pause'],
  ['QUEUED', 'waiting', 'pause'],
  ['PAUSED', 'paused', 'resume'],
] as const)('shows %s scan controls before stale unconfigured volume actions', (state, label, action) => {
  const view = renderView(VolumeCard, {
    volume: volume('UNCONFIGURED'), title: 'Fixture volume', scanDisabled: false,
    scanProgress: { volumeId: 'fixture', state, indexed: 76846, pending: 1, total: null },
  }, components)
  try {
    expect(button(view.root, 'file_index.create')).toBeUndefined()
    expect(button(view.root, label)?.props.disabled).toBe(true)
    expect(button(view.root, action)).toBeDefined()
    expect(button(view.root, 'stop')).toBeDefined()
  } finally { view.unmount() }
})
