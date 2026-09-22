import type { IStorageMount } from '@/lib/interfaces'

export type TranslateFn = (key: string, params?: Record<string, any>) => string

/** Base display title for a NAS volume/partition, ignoring the alias.
 *  Port of old_web `getStorageVolumeBaseTitle`. */
export function getStorageVolumeBaseTitle(v: IStorageMount, t: TranslateFn): string {
  const mountPoint = String(v?.mountPoint ?? '').trim()
  // For the root mount we show a localized name.
  if (mountPoint === '/') return t('internal_storage')

  const path = String(v?.path ?? '').trim()
  const label = String(v?.label ?? '').trim()
  if (label) return label

  const fs = String(v?.fsType ?? '').trim().toLowerCase()
  const size = Number(v?.totalBytes ?? 0)
  if (fs === 'vfat' && size > 0 && size < 2 * 1000 * 1000 * 1000) return t('efi_partition')

  if (typeof v?.partitionNum === 'number') return t('partition_x', { n: v.partitionNum })
  return String(v?.name ?? '').trim() || path
}

/** Display title with the user alias taking precedence. */
export function getStorageVolumeTitle(v: IStorageMount, t: TranslateFn): string {
  const alias = String(v?.alias ?? '').trim()
  return alias ? alias : getStorageVolumeBaseTitle(v, t)
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
