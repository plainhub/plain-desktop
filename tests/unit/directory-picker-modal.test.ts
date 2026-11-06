import { beforeEach, expect, it, vi } from 'vitest'
import { createSSRApp, ref } from 'vue'
import { renderToString } from 'vue/server-renderer'
import DirectoryPickerModal from '@/components/DirectoryPickerModal.vue'
import VListItem from '@/components/base/VListItem.vue'
import VTextButton from '@/components/base/VTextButton.vue'
import VFilledButton from '@/components/base/VFilledButton.vue'
import VModal from '@/components/base/VModal.vue'
import VCircularProgress from '@/components/base/VCircularProgress.vue'
import { popModal } from '@/components/modal'
import type { DirectoryPickerVolume } from '@/hooks/directory-picker'
import tooltip from '@/plugins/tooltip'

const mock = vi.hoisted(() => ({ picker: null as any }))
vi.mock('@/hooks/directory-picker', () => ({ useDirectoryPicker: () => mock.picker }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('@/components/modal', () => ({ Modal: { EVENT_PROMPT: 'prompt' }, popModal: vi.fn() }))

beforeEach(() => {
  vi.clearAllMocks()
  mock.picker = {
    volumes: ref([
      { id: 'root', name: 'System disk', mountPoint: '/', totalBytes: 0 },
      { id: 'archive', label: 'Archive', mountPoint: '/media/archive', totalBytes: 0 },
    ]),
    loadingMounts: ref(false), rootPath: ref('/media/archive'), currentDir: ref('/media/archive/Reports'),
    listing: ref(false), listingError: ref(''),
    dirItems: ref([{ path: '/media/archive/Reports/Invoices', isSystem: false }]),
    dirName: (path: string) => path.split('/').pop(),
    selectRoot: vi.fn(), selectDirectory: vi.fn(), enterDir: vi.fn(), reload: vi.fn(),
  }
})

async function view(recentDirs: string[] = [], volume?: DirectoryPickerVolume) {
  let bindings!: { chooseCurrent(): void; cancel(): void; selectDirectory(path: string): void }
  const selected = vi.fn()
  const component = DirectoryPickerModal as any
  const app = createSSRApp({ ...component, setup(props: unknown, context: unknown) {
    bindings = component.setup(props, context)
    return bindings
  } }, { recentDirs, volume, onPrompt: selected })
  app.config.globalProperties.$t = (key: string) => key
  app.use(tooltip)
  app.component('VModal', VModal)
  app.component('VListItem', VListItem)
  app.component('VTextButton', VTextButton)
  app.component('VFilledButton', VFilledButton)
  app.component('VCircularProgress', VCircularProgress)
  const context = { teleports: {} as Record<string, string> }
  await renderToString(app, context)
  const html = context.teleports.body
  return { html, bindings, selected }
}

it('shows named volumes and one complete current selection with one explicit confirmation action', async () => {
  const { html } = await view()
  expect(html).toContain('System disk')
  expect(html).toContain('Archive')
  expect(html).toContain('Reports')
  expect(html).toContain('aria-current="page"')
  expect(html).toContain('folder_picker_current')
  expect(html.match(/value="ok"/g)).toHaveLength(1)
  expect(html).toContain('select_this_folder')
  expect(html).toContain('folder_picker_hint')
  expect(html).not.toContain('picker-current__value')
  expect(html).not.toContain('parent_folder')
})

it('emits the unchanged absolute directory only when the user confirms', async () => {
  const { bindings, selected } = await view()
  expect(selected).not.toHaveBeenCalled()
  bindings.chooseCurrent()
  expect(selected).toHaveBeenCalledExactlyOnceWith('/media/archive/Reports')
})

it('shows only the specified volume path without presenting a volume list', async () => {
  const { html } = await view([], { mountPoint: '/media/archive', name: 'Archive' })
  expect(html).toContain('Archive')
  expect(html).toContain('Reports')
  expect(html).toContain('aria-current="page"')
  expect(html).not.toContain('System disk')
  expect(html).not.toContain('browser-sidebar')
  expect(html).not.toContain('aria-pressed')
})

it('opens recent directories for review before confirmation and preserves cancellation', async () => {
  const { html, bindings, selected } = await view(['/media/archive/Backups'])
  expect(html).toContain('recent_folders')
  bindings.selectDirectory('/media/archive/Backups')
  expect(mock.picker.selectDirectory).toHaveBeenCalledExactlyOnceWith('/media/archive/Backups')
  expect(selected).not.toHaveBeenCalled()
  mock.picker.currentDir.value = '/media/archive/Backups'
  bindings.chooseCurrent()
  expect(selected).toHaveBeenCalledExactlyOnceWith('/media/archive/Backups')
  bindings.cancel()
  expect(popModal).toHaveBeenCalledOnce()
})

it.each(['mounts', 'directory', 'failure'])('disables confirmation during %s and hides stale folder rows', async state => {
  mock.picker.loadingMounts.value = state === 'mounts'
  mock.picker.listing.value = state === 'directory'
  mock.picker.listingError.value = state === 'failure' ? 'access_denied' : ''
  const { html } = await view()
  const confirm = html.match(/<button[^>]*value="ok"[^>]*>/)?.[0]
  expect(confirm).toContain('disabled')
  expect(html).not.toContain('Invoices')
  if (state === 'failure') {
    expect(html).toContain('access_denied')
    expect(html).toContain('retry')
    expect(html).not.toContain('folder_picker_empty')
  }
})

it('labels the volume root and displays ordinary folders without the system-and-apps group', async () => {
  mock.picker.rootPath.value = '/'
  mock.picker.currentDir.value = '/'
  mock.picker.dirItems.value = [{ path: '/System', isSystem: true }, { path: '/Users', isSystem: false }]
  const { html } = await view()
  expect(html).toContain('System disk')
  expect(html).toContain('aria-current="page"')
  expect(html).toContain('Users')
  expect(html).not.toContain('system_and_apps')
  expect(html).not.toContain('title="System"')
  expect(html).not.toContain('aria-expanded')
})

it('shows the empty-folder guidance when only system directories remain', async () => {
  mock.picker.dirItems.value = [{ path: '/System', isSystem: true }]
  const { html } = await view()
  expect(html).toContain('folder_picker_empty')
  expect(html).not.toContain('system_and_apps')
})

it('explains that a folder without children can still be selected', async () => {
  mock.picker.dirItems.value = []
  const { html } = await view()
  expect(html).toContain('folder_picker_empty')
  expect(html.match(/<button[^>]*value="ok"[^>]*>/)?.[0]).not.toContain('disabled')
})
