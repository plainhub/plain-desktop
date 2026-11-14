import type { FileIndexVolume } from '@/lib/api/file-index'
import type { IStorageMount } from '@/lib/interfaces'
import { sortMounts } from '@/lib/storage'

export function mountedIndexVolumes(mounts: IStorageMount[], volumes: FileIndexVolume[]) {
  const current = new Map(volumes.filter((volume) => volume.isAvailable).map((volume) => [volume.path, volume]))
  return sortMounts(mounts).flatMap((mount) => {
    const volume = mount.mountPoint && current.get(mount.mountPoint)
    return volume ? [{ mount, volume }] : []
  })
}
