import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { effectScope, nextTick } from 'vue'
import type { IStorageMount } from '@/lib/interfaces'

const query = vi.hoisted(() => ({ mounts: null as any, files: null as any, loading: null as any, fetch: vi.fn(), refetch: vi.fn() }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('@/components/toaster', () => ({ default: vi.fn() }))
vi.mock('@/lib/api/file', () => ({ getFileName: (path: string) => path.split('/').pop() }))
vi.mock('@/lib/api/query', async () => {
  const { ref } = await import('vue')
  return {
    mountsGQL: 'mounts', directoryEntriesGQL: 'directoryEntries',
    initQuery: (options: unknown) => { query.mounts = options; return { refetch: query.refetch } },
    initLazyQuery: (options: unknown) => {
      query.files = options
      query.loading = ref(false)
      return { loading: query.loading, fetch: query.fetch }
    },
  }
})
import { useDirectoryPicker, type DirectoryPickerVolume } from '@/hooks/directory-picker'

const mounts = [
  { id: 'root', mountPoint: '/', name: 'System disk' },
  { id: 'archive', mountPoint: '/media/archive', name: 'Archive' },
  { id: 'nested', mountPoint: '/media/archive/photos', name: 'Photos' },
] as IStorageMount[]
let scope: ReturnType<typeof effectScope>
beforeEach(() => {
  vi.clearAllMocks()
  scope = effectScope()
  query.fetch.mockImplementation(async () => { query.loading.value = true })
})
afterEach(() => scope.stop())
function create(initialPath = '', volume?: DirectoryPickerVolume) { return scope.run(() => useDirectoryPicker(initialPath, volume))! }
async function loadMounts() { query.mounts.handle({ mounts }, ''); await nextTick() }

it('opens a specified volume directly without loading the volume list and stays inside its root', async () => {
  const picker = create('/media/archive/Reports', { mountPoint: '/media/archive', name: 'Archive' })
  expect(query.mounts.enabled()).toBe(false)
  expect(picker.loadingMounts.value).toBe(false)
  expect(picker.rootPath.value).toBe('/media/archive')
  expect(picker.currentDir.value).toBe('/media/archive/Reports')
  expect(query.files.variables().root).toBe('/media/archive/Reports')
  picker.enterDir(picker.rootPath.value)
  expect(picker.currentDir.value).toBe('/media/archive')
  picker.selectDirectory('/media/archive-old/Reports')
  expect(picker.currentDir.value).toBe('/media/archive')
  expect(query.refetch).not.toHaveBeenCalled()
})

it('loads the root directory after initial mount selection even though the displayed path was already root', async () => {
  const picker = create()
  expect(picker.loadingMounts.value).toBe(true)
  expect(query.fetch).not.toHaveBeenCalled()
  await loadMounts()
  expect(picker.rootPath.value).toBe('/')
  expect(query.files.variables().root).toBe('/')
  expect(query.files.document).toBe('directoryEntries')
  expect(query.fetch).toHaveBeenCalledExactlyOnceWith(undefined, { latest: true })
})

it('selects the deepest matching volume for an initial directory and keeps absolute return paths', async () => {
  const picker = create('/media/archive/photos/Trips/')
  await loadMounts()
  expect(picker.rootPath.value).toBe('/media/archive/photos')
  expect(picker.currentDir.value).toBe('/media/archive/photos/Trips')
  picker.enterDir(picker.rootPath.value)
  expect(picker.currentDir.value).toBe('/media/archive/photos')
})

it('opens a recent directory on another volume without submitting or keeping the previous volume path', async () => {
  const picker = create()
  await loadMounts()
  picker.selectDirectory('/media/archive/Reports')
  expect(picker.rootPath.value).toBe('/media/archive')
  expect(picker.currentDir.value).toBe('/media/archive/Reports')
  picker.selectRoot('/')
  expect(picker.currentDir.value).toBe('/')
  picker.selectDirectory('/media/archive-old/Reports')
  expect(picker.rootPath.value).toBe('/')
  expect(picker.currentDir.value).toBe('/media/archive-old/Reports')
})

it('clears stale folders before fetching the newly selected directory with latest-request semantics', async () => {
  const picker = create()
  await loadMounts()
  query.files.handle({ files: [{ path: '/Users', isDir: true }, { path: '/file.txt', isDir: false }] }, '')
  expect(picker.dirItems.value.map(item => item.path)).toEqual(['/Users'])
  picker.selectRoot('/media/archive')
  await nextTick()
  expect(picker.dirItems.value).toEqual([])
  expect(query.files.variables().root).toBe('/media/archive')
  expect(query.fetch).toHaveBeenLastCalledWith(undefined, { latest: true })
})

it('preserves the user-selected directory when volume names refresh instead of reapplying the initial directory', async () => {
  const picker = create('/media/archive/Reports')
  await loadMounts()
  picker.selectDirectory('/media/archive/photos/Trips')
  query.mounts.handle({ mounts: mounts.map(volume => ({ ...volume, label: 'Updated disk label' })) }, '')
  expect(picker.rootPath.value).toBe('/media/archive/photos')
  expect(picker.currentDir.value).toBe('/media/archive/photos/Trips')
  expect(picker.volumes.value.every(volume => volume.label === 'Updated disk label')).toBe(true)
})

it('keeps failed listings distinct from selectable empty folders and retries the same directory', async () => {
  const picker = create('/media/archive/Reports')
  await loadMounts()
  query.files.handle(undefined, 'access_denied')
  expect(picker.listingError.value).toBe('access_denied')
  expect(picker.dirItems.value).toEqual([])
  await picker.reload()
  expect(picker.currentDir.value).toBe('/media/archive/Reports')
  expect(picker.listingError.value).toBe('')
  expect(query.files.variables().root).toBe('/media/archive/Reports')
  query.files.handle({ files: [] }, '')
  expect(picker.dirItems.value).toEqual([])
  expect(picker.listingError.value).toBe('')
})

it('retries mount loading before requesting directories after a mount failure', async () => {
  const picker = create()
  query.mounts.handle(undefined, 'network_error')
  expect(picker.loadingMounts.value).toBe(false)
  expect(picker.listingError.value).toBe('network_error')
  await picker.reload()
  expect(picker.loadingMounts.value).toBe(true)
  expect(query.refetch).toHaveBeenCalledOnce()
  expect(query.fetch).not.toHaveBeenCalled()
})
