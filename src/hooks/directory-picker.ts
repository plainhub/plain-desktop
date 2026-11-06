import { computed, ref, watch } from 'vue'
import toast from '@/components/toaster'
import { useI18n } from 'vue-i18n'
import { directoryEntriesGQL, initLazyQuery, initQuery, mountsGQL } from '@/lib/api/query'
import type { IStorageMount } from '@/lib/interfaces'
import type { IFile } from '@/lib/file'
import { getFileName } from '@/lib/api/file'
import { compareLocale } from '@/lib/array'

export type DirectoryPickerVolume = Pick<IStorageMount, 'mountPoint' | 'name'>

export function useDirectoryPicker(initialPath: string, volume?: DirectoryPickerVolume) {
  const { t } = useI18n()

  const volumes = ref<IStorageMount[]>([])
  const loadingMounts = ref(!volume)
  const rootPath = ref(volume?.mountPoint || '')
  const relativePath = ref('')
  const dirItems = ref<Pick<IFile, 'path' | 'isDir' | 'isSystem'>[]>([])
  const listingError = ref('')

  const normalizedInitialPath = computed(() => {
    const p = String(initialPath || '').trim()
    return p ? p.replace(/\/+$/g, '') || '/' : ''
  })

  const currentDir = computed(() => {
    const root = rootPath.value || '/'
    const rel = (relativePath.value || '').replace(/^\/+/, '')
    if (!rel) return root
    if (root === '/') return `/${rel}`.replace(/\/+/g, '/').replace(/\/+$/g, '')
    return `${root}/${rel}`.replace(/\/+/g, '/').replace(/\/+$/g, '')
  })

  function selectRoot(mountPoint: string) { rootPath.value = mountPoint || '/'; relativePath.value = '' }

  function selectDirectory(absPath: string) {
    const dir = absPath.trim().replace(/\/+$/g, '') || '/'
    let best = ''
    for (const candidate of volume ? [volume] : volumes.value) {
      const mount = String(candidate.mountPoint || '').trim()
      if (mount && (dir === mount || dir.startsWith(mount === '/' ? '/' : mount + '/')) && mount.length > best.length) best = mount
    }
    if (!best) { toast(t('invalid_file_path'), 'error'); return }
    rootPath.value = best
    relativePath.value = normalizeRelativeFromAbs(dir)
  }

  function normalizeRelativeFromAbs(absPath: string) {
    const root = rootPath.value || ''
    if (!root) return ''
    if (root === '/') return absPath.replace(/^\/+/, '')
    if (!absPath.startsWith(root)) return ''
    return absPath.slice(root.length).replace(/^\/+/, '')
  }

  function enterDir(absPath: string) { relativePath.value = normalizeRelativeFromAbs(absPath) }

  function dirName(path: string) { return getFileName(path) || path }

  const { refetch: fetchMounts } = initQuery<{ mounts: IStorageMount[] }>({
    document: mountsGQL,
    enabled: () => !volume,
    handle: (data, error) => {
      loadingMounts.value = false
      listingError.value = error
      if (error) return
      volumes.value = [...(data?.mounts ?? [])].sort((a, b) => {
        const am = String(a?.mountPoint ?? ''), bm = String(b?.mountPoint ?? '')
        if (am === '/') return -1; if (bm === '/') return 1
        return compareLocale(am, bm, { numeric: true })
      })
      if (!rootPath.value) {
        selectRoot(volumes.value[0]?.mountPoint || '/')
        if (normalizedInitialPath.value) selectDirectory(normalizedInitialPath.value)
      }
    },
  })

  const { loading: listing, fetch: fetchDirs } = initLazyQuery<{ files: Pick<IFile, 'path' | 'isDir' | 'isSystem'>[] }>({
    document: directoryEntriesGQL,
    variables: () => ({ root: currentDir.value, offset: 0, limit: 10000, query: '', sortBy: 'NAME_ASC' }),
    handle: (data, error) => {
      listingError.value = error
      if (error) { dirItems.value = []; return }
      dirItems.value = (data?.files ?? []).filter((f) => f.isDir)
    },
  })

  function reload() {
    dirItems.value = []
    listingError.value = ''
    if (rootPath.value) return fetchDirs(undefined, { latest: true })
    loadingMounts.value = true
    return fetchMounts()
  }

  if (volume && normalizedInitialPath.value) selectDirectory(normalizedInitialPath.value)
  watch([rootPath, relativePath], () => { if (rootPath.value) void reload() }, { immediate: true })

  return { volumes, loadingMounts, rootPath, currentDir, listing, listingError, dirItems, selectRoot, selectDirectory, enterDir, dirName, reload }
}
