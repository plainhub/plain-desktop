import { computed, ref, watch } from 'vue'
import toast from '@/components/toaster'
import { useI18n } from 'vue-i18n'
import { directoryEntriesGQL, initLazyQuery, initQuery, mountsGQL } from '@/lib/api/query'
import type { IStorageMount } from '@/lib/interfaces'
import type { IFile } from '@/lib/file'
import { getFileName } from '@/lib/api/file'
import { compareLocale } from '@/lib/array'
import { directoryWithinRoot, findDirectoryVolume, normalizeDirectoryPath } from '@/lib/directory-picker-path'

export type DirectoryPickerVolume = Pick<IStorageMount, 'mountPoint' | 'name'>

export function useDirectoryPicker(initialPath: string, volume?: DirectoryPickerVolume) {
  const { t } = useI18n()

  const volumes = ref<IStorageMount[]>([])
  const loadingMounts = ref(!volume)
  const rootPath = ref(normalizeDirectoryPath(volume?.mountPoint ?? ''))
  const selectedPath = ref('')
  const dirItems = ref<Pick<IFile, 'path' | 'isDir' | 'isSystem'>[]>([])
  const listingError = ref('')

  const firstPath = normalizeDirectoryPath(initialPath)
  const currentDir = computed(() => selectedPath.value || rootPath.value || '/')

  function selectRoot(mountPoint: string) {
    rootPath.value = normalizeDirectoryPath(mountPoint) || '/'
    selectedPath.value = ''
  }

  function selectDirectory(absPath: string) {
    const dir = normalizeDirectoryPath(absPath)
    const volumeMatch = findDirectoryVolume(dir, volume ? [volume] : volumes.value)
    if (!volumeMatch) { toast(t('invalid_file_path'), 'error'); return }
    rootPath.value = normalizeDirectoryPath(volumeMatch.mountPoint)
    selectedPath.value = dir
  }

  function enterDir(absPath: string) {
    if (!directoryWithinRoot(absPath, rootPath.value)) {
      toast(t('invalid_file_path'), 'error')
      return
    }
    selectedPath.value = normalizeDirectoryPath(absPath)
  }

  function dirName(path: string) { return getFileName(path) || path }

  const { refetch: fetchMounts } = initQuery<{ mounts: IStorageMount[] }>({
    document: mountsGQL,
    enabled: () => !volume,
    handle: (data, error) => {
      loadingMounts.value = false
      listingError.value = error
      if (error) return
      volumes.value = (data?.mounts ?? []).map((item) => ({
        ...item, mountPoint: normalizeDirectoryPath(item.mountPoint || ''),
      })).sort((a, b) => {
        const am = String(a?.mountPoint ?? ''), bm = String(b?.mountPoint ?? '')
        if (am === '/') return -1; if (bm === '/') return 1
        return compareLocale(am, bm, { numeric: true })
      })
      if (!rootPath.value) {
        selectRoot(volumes.value[0]?.mountPoint || '/')
        if (firstPath) selectDirectory(firstPath)
      }
    },
  })

  const { loading: listing, fetch: fetchDirs } = initLazyQuery<{ files: Pick<IFile, 'path' | 'isDir' | 'isSystem'>[] }>({
    document: directoryEntriesGQL,
    variables: () => ({ root: currentDir.value, offset: 0, limit: 10000, query: '', sortBy: 'NAME_ASC' }),
    handle: (data, error) => {
      listingError.value = error
      if (error) { dirItems.value = []; return }
      dirItems.value = (data?.files ?? []).filter((f) => f.isDir).map((f) => ({ ...f, path: normalizeDirectoryPath(f.path) }))
    },
  })

  function reload() {
    dirItems.value = []
    listingError.value = ''
    if (rootPath.value) return fetchDirs(undefined, { latest: true })
    loadingMounts.value = true
    return fetchMounts()
  }

  if (volume && firstPath) selectDirectory(firstPath)
  watch([rootPath, currentDir], () => { if (rootPath.value) void reload() }, { immediate: true })

  return { volumes, loadingMounts, rootPath, currentDir, listing, listingError, dirItems, selectRoot, selectDirectory, enterDir, dirName, reload }
}
