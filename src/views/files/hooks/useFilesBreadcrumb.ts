import { computed, type Ref } from 'vue'
import { getStorageVolumeTitle } from '@/lib/volumes'
import { formatFileSize } from '@/lib/format'
import { getFileName } from '@/lib/api/file'
import { isZipPath, getZipFilePath, getZipInternalPath, ZIP_SEPARATOR, getDirectoryBreadcrumbs } from '@/lib/file'
import type { IFileFilter, IStorageMount } from '@/lib/interfaces'

export function useFilesBreadcrumb(
  filter: IFileFilter,
  rootDir: Ref<string>,
  mounts: Ref<IStorageMount[]>,
  t: (key: string, args?: any) => string,
) {
  function getPageTitle() {
    const mount = mounts.value.find((m) => m.mountPoint === filter.rootPath)
    return mount ? getStorageVolumeTitle(mount, t) : filter.rootPath
  }

  function getPageStats() {
    if (filter.type === 'APP') return t('app_data')
    const v = mounts.value.find((m: IStorageMount) => m.mountPoint === filter.rootPath)
    if (!v) return ''
    return t('storage_free_total', {
      free: formatFileSize(v.freeBytes ?? 0),
      total: formatFileSize(v.totalBytes ?? 0),
    })
  }

  const breadcrumbPaths = computed(() => {
    const parent = filter.parent

    if (isZipPath(parent)) {
      const zipFilePath = getZipFilePath(parent)
      const internalPath = getZipInternalPath(parent)

      const paths = getDirectoryBreadcrumbs(zipFilePath.substring(0, zipFilePath.lastIndexOf('/')), rootDir.value, getPageTitle())
      paths.push({ path: zipFilePath + ZIP_SEPARATOR, name: getFileName(zipFilePath) })

      const segments = internalPath.split('/').filter((s) => s.length > 0)
      let currentPath = zipFilePath + ZIP_SEPARATOR
      for (const segment of segments) {
        currentPath += segment + '/'
        paths.push({ path: currentPath, name: segment })
      }
      return paths
    }

    return getDirectoryBreadcrumbs(parent, rootDir.value, getPageTitle())
  })

  return { breadcrumbPaths, getPageStats }
}
