import type { IStorageMount } from '@/lib/interfaces'
import { DriveType } from '@/lib/status'
import { formatFileSize } from '@/lib/format'
import type { ComposerTranslation } from 'vue-i18n'
import { compareLocale } from '@/lib/array'

export function driveRank(m: IStorageMount): number {
  if (m.driveType === DriveType.INTERNAL_STORAGE) return 0
  if (m.driveType === DriveType.SDCARD) return 1
  if (m.driveType === DriveType.USB_STORAGE) return 2
  if (m.driveType === DriveType.APP) return 3
  return 9
}

export function sortMounts(mounts: IStorageMount[]): IStorageMount[] {
  return [...mounts].sort((a, b) => {
    const da = driveRank(a)
    const db = driveRank(b)
    if (da !== db) return da - db
    return compareLocale(a.mountPoint || '', b.mountPoint || '')
  })
}

export function storageUsedPercent(freeBytes: number, totalBytes: number): number {
  if (totalBytes <= 0) return 0
  const usedBytes = Math.max(0, totalBytes - freeBytes)
  const pct = (usedBytes / totalBytes) * 100
  return Number.isFinite(pct) ? Math.max(0, Math.min(100, pct)) : 0
}

export function storageCountText(freeBytes: number, totalBytes: number, t: ComposerTranslation): string {
  if (totalBytes <= 0) return ''
  return t('storage_free_total', {
    free: formatFileSize(freeBytes),
    total: formatFileSize(totalBytes),
  })
}
