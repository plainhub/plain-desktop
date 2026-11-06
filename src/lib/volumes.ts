import { formatFileSize } from '@/lib/format'
import { DriveType } from '@/lib/status'
import type { IStorageMount } from '@/lib/interfaces'

export type TranslateFn = (key: string, params?: Record<string, any>) => string

export function getStorageVolumeTitle(v: IStorageMount, t: TranslateFn): string {
  const alias = String(v.alias ?? '').trim()
  if (alias) return alias
  const mountPoint = String(v?.mountPoint ?? '').trim()
  const path = String(v?.path ?? '').trim()
  const label = String(v?.label ?? '').trim()
  if (label) return label
  const name = String(v.name ?? '').trim()
  if (mountPoint === '/') return name && name !== '/' ? name : t('internal_storage')

  const fs = String(v?.fsType ?? '').trim().toLowerCase()
  const size = Number(v?.totalBytes ?? 0)
  if (fs === 'vfat' && size > 0 && size < 2 * 1000 * 1000 * 1000) return t('efi_partition')

  if (typeof v?.partitionNum === 'number') return t('partition_x', { n: v.partitionNum })
  const model = String(v.diskModel ?? '').trim()
  if (model) return size > 0 ? `${model} (${formatFileSize(size)})` : model
  if (name) return name
  if (v.driveType === DriveType.APP) return t('app_data')
  if (v.driveType === DriveType.SDCARD) return t('sdcard')
  if (v.driveType === DriveType.INTERNAL_STORAGE) return t('internal_storage')
  return mountPoint || path || t('usb_storage')
}

function isInternalStorageVolume(v: IStorageMount): boolean {
  const name = String(v?.name ?? '').trim()
  const mountPoint = String(v?.mountPoint ?? '').trim()
  return name === '/' || mountPoint === '/'
}

/** Sort by what the UI shows (title), internal storage first. */
export function sortStorageVolumesByTitle(volumes: IStorageMount[], t: TranslateFn): IStorageMount[] {
  return [...(volumes ?? [])].sort((a, b) => {
    const ai = isInternalStorageVolume(a)
    const bi = isInternalStorageVolume(b)
    if (ai !== bi) return ai ? -1 : 1

    const at = getStorageVolumeTitle(a, t)
    const bt = getStorageVolumeTitle(b, t)

    const byTitle = at.localeCompare(bt, undefined, { numeric: true, sensitivity: 'base' })
    if (byTitle !== 0) return byTitle

    const am = String(a?.mountPoint ?? '').trim()
    const bm = String(b?.mountPoint ?? '').trim()
    return am.localeCompare(bm, undefined, { numeric: true, sensitivity: 'base' })
  })
}
