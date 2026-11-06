import { expect, it, vi } from 'vitest'
import { ref } from 'vue'
vi.mock('@/lib/prefs/appearance', () => ({ get: (_key: string, fallback: unknown) => fallback }))
vi.mock('@/lib/api/file', () => ({ getFileName: (path: string) => path.split('/').pop() }))
import { useFilesBreadcrumb } from '@/views/files/hooks/useFilesBreadcrumb'
import type { IFileFilter, IStorageMount } from '@/lib/interfaces'
import { getDirectoryBreadcrumbs } from '@/lib/file'

it('builds clickable ancestors inside the selected disk and preserves complete Unicode names', () => {
  const name = '年度工作资料与归档文件夹📁'
  expect(getDirectoryBreadcrumbs(`/media/archive/${name}/Reports`, '/media/archive', 'Archive')).toEqual([
    { path: '/media/archive', name: 'Archive' },
    { path: `/media/archive/${name}`, name },
    { path: `/media/archive/${name}/Reports`, name: 'Reports' },
  ])
  expect(getDirectoryBreadcrumbs('/media/archive', '/media/archive', 'Archive')).toEqual([{ path: '/media/archive', name: 'Archive' }])
  expect(getDirectoryBreadcrumbs('/Users/Shared', '/', 'System disk')).toEqual([
    { path: '/', name: 'System disk' }, { path: '/Users', name: 'Users' }, { path: '/Users/Shared', name: 'Shared' },
  ])
})

it('uses current mount metadata when the route still contains an outdated drive type', () => {
  const filter = { type: 'INTERNAL_STORAGE', rootPath: '/mnt/usb2', parent: '/mnt/usb2' } as IFileFilter
  const mounts = ref([
    { mountPoint: '/', driveType: 'INTERNAL_STORAGE' },
    { mountPoint: '/mnt/usb1', driveType: 'USB_STORAGE' },
    { mountPoint: '/mnt/usb2', driveType: 'USB_STORAGE', diskModel: 'Example Drive', totalBytes: 314e9 },
  ] as IStorageMount[])
  const { breadcrumbPaths } = useFilesBreadcrumb(filter, ref('/mnt/usb2'), mounts, (key) => key)
  expect(breadcrumbPaths.value).toEqual([{ path: '/mnt/usb2', name: 'Example Drive (314.0 GB)' }])
  mounts.value[2].alias = 'Archive'
  expect(breadcrumbPaths.value[0].name).toBe('Archive')
})

it('preserves archive boundaries and complete internal paths in shared breadcrumbs', () => {
  const filter = { type: 'INTERNAL_STORAGE', rootPath: '/archive', parent: '/archive/Backups/photos.zip!zip!/Trips/2026/' } as IFileFilter
  const mounts = ref([{ mountPoint: '/archive', label: 'Archive' }] as IStorageMount[])
  const { breadcrumbPaths } = useFilesBreadcrumb(filter, ref('/archive'), mounts, (key) => key)
  expect(breadcrumbPaths.value).toEqual([
    { path: '/archive', name: 'Archive' },
    { path: '/archive/Backups', name: 'Backups' },
    { path: '/archive/Backups/photos.zip!zip!/', name: 'photos.zip' },
    { path: '/archive/Backups/photos.zip!zip!/Trips/', name: 'Trips' },
    { path: '/archive/Backups/photos.zip!zip!/Trips/2026/', name: '2026' },
  ])
})
