import { expect, it, vi } from 'vitest'
import { ref } from 'vue'
vi.mock('@/lib/prefs/appearance', () => ({ get: (_key: string, fallback: unknown) => fallback }))
vi.mock('@/lib/api/file', () => ({ getFileName: (path: string) => path.split('/').pop() }))
import { useFilesBreadcrumb } from '@/views/files/hooks/useFilesBreadcrumb'
import type { IFileFilter, IStorageMount } from '@/lib/interfaces'

it('uses current mount metadata when the route still contains an outdated drive type', () => {
  const filter = { type: 'INTERNAL_STORAGE', rootPath: '/mnt/usb2', parent: '/mnt/usb2' } as IFileFilter
  const mounts = ref([
    { mountPoint: '/', driveType: 'INTERNAL_STORAGE' },
    { mountPoint: '/mnt/usb1', driveType: 'USB_STORAGE' },
    { mountPoint: '/mnt/usb2', driveType: 'USB_STORAGE' },
  ] as IStorageMount[])
  const { breadcrumbPaths } = useFilesBreadcrumb(filter, ref('/mnt/usb2'), mounts, (key) => key)
  expect(breadcrumbPaths.value).toEqual([{ path: '/mnt/usb2', name: 'usb_storage 2' }])
  mounts.value[2].driveType = 'INTERNAL_STORAGE'
  expect(breadcrumbPaths.value[0].name).toBe('internal_storage')
})
