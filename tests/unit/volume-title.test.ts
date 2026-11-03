import { expect, it, vi } from 'vitest'
vi.mock('@/lib/prefs/appearance', () => ({ get: (_key: string, fallback: unknown) => fallback }))
import { getStorageVolumeTitle } from '@/lib/volumes'
import type { IStorageMount } from '@/lib/interfaces'

const t = (key: string) => key
const volume = { mountPoint: '/mnt/usb2', name: 'usb2', driveType: 'USB_STORAGE', totalBytes: 314e9, diskModel: 'Example Drive' } as IStorageMount

it('uses the disk model and volume capacity when there is no volume label', () => {
  expect(getStorageVolumeTitle(volume, t)).toBe('Example Drive (314.0 GB)')
})
it('prefers the volume label and then the user alias', () => {
  expect(getStorageVolumeTitle({ ...volume, label: ' Photos ' }, t)).toBe('Photos')
  expect(getStorageVolumeTitle({ ...volume, label: 'Photos', alias: ' Archive ' }, t)).toBe('Archive')
})
it('preserves platform volume names and the unnamed root title', () => {
  expect(getStorageVolumeTitle({ ...volume, diskModel: '', name: 'External Media' }, t)).toBe('External Media')
  expect(getStorageVolumeTitle({ ...volume, mountPoint: '/' }, t)).toBe('internal_storage')
})
it('does not append an empty capacity to the model', () => {
  expect(getStorageVolumeTitle({ ...volume, totalBytes: 0 }, t)).toBe('Example Drive')
})
