import { computed, type Ref } from 'vue'
import { formatFileSize } from '@/lib/format'
import { getFileName } from '@/lib/api/file'
import { isZipPath, getZipFilePath, getZipInternalPath, ZIP_SEPARATOR } from '@/lib/file'
import type { IFileFilter, IBreadcrumbItem, IStorageMount } from '@/lib/interfaces'

export function useFilesBreadcrumb(
  filter: IFileFilter,
  rootDir: Ref<string>,
  mounts: Ref<IStorageMount[]>,
  t: (key: string, args?: any) => string,
) {
  function getPageTitle() {
    if (filter.type === 'SDCARD') return t('sdcard')
    if (filter.type === 'APP') return t('app_data')
    if (filter.type === 'USB_STORAGE') {
      const usbRoots = mounts.value.filter((m: IStorageMount) => m.driveType === 'USB_STORAGE').map((m: IStorageMount) => m.mountPoint)
      const usbIndex = usbRoots.indexOf(filter.rootPath)
      return `${t('usb_storage')} ${usbIndex !== -1 ? usbIndex + 1 : 1}`
    }
    return t('internal_storage')
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
    const paths: IBreadcrumbItem[] = []
    const parent = filter.parent

    if (isZipPath(parent)) {
      const zipFilePath = getZipFilePath(parent)
      const internalPath = getZipInternalPath(parent)

      let p = zipFilePath.substring(0, zipFilePath.lastIndexOf('/'))
      while (p) {
        if (p === rootDir.value) break
        paths.unshift({ path: p, name: getFileName(p) })
        p = p.substring(0, p.lastIndexOf('/'))
      }
      paths.unshift({ path: rootDir.value, name: getPageTitle() })
      paths.push({ path: zipFilePath + ZIP_SEPARATOR, name: getFileName(zipFilePath) })

      const segments = internalPath.split('/').filter((s) => s.length > 0)
      let currentPath = zipFilePath + ZIP_SEPARATOR
      for (const segment of segments) {
        currentPath += segment + '/'
        paths.push({ path: currentPath, name: segment })
      }
      return paths
    }

    let p = parent
    while (p) {
      if (p === rootDir.value) break
      paths.unshift({ path: p, name: getFileName(p) })
      p = p.substring(0, p.lastIndexOf('/'))
    }
    paths.unshift({ path: rootDir.value, name: getPageTitle() })
    return paths
  })

  return { breadcrumbPaths, getPageStats }
}
