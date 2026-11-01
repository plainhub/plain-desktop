import { expect, it, vi } from 'vitest'
import { effectScope, nextTick, ref } from 'vue'
import { useSystemFiles } from '@/views/files/hooks/useSystemFiles'
import type { IFile } from '@/lib/file'
vi.mock('@/lib/api/mutation', () => ({ initMutation: vi.fn() }))
vi.mock('@/components/toaster', () => ({ default: vi.fn() }))
import { useSelectable } from '@/hooks/list'

it('groups system entries only during browsing and keeps collapsed entries out of selection order', async () => {
  const scope = effectScope()
  const files = ref([
    { id: 'home', isSystem: false },
    { id: 'usr', isSystem: true },
    { id: 'data', isSystem: false },
    { id: 'etc', isSystem: true },
  ] as IFile[])
  const directory = ref('/')
  const browsing = ref(true)
  const group = scope.run(() => useSystemFiles(files, () => directory.value, () => browsing.value))!
  expect(group.systemItems.value.map(item => item.id)).toEqual(['usr', 'etc'])
  expect(group.visibleItems.value.map(item => item.id)).toEqual(['home', 'data'])
  const selection = useSelectable(group.visibleItems)
  selection.selectAll()
  expect(selection.selectedIds.value).toEqual(['home', 'data'])
  group.expanded.value = true
  expect(group.visibleItems.value.map(item => item.id)).toEqual(['usr', 'etc', 'home', 'data'])
  selection.selectAll()
  expect(selection.selectedIds.value).toEqual(['usr', 'etc', 'home', 'data'])
  browsing.value = false
  expect(group.systemItems.value).toEqual([])
  expect(group.visibleItems.value).toEqual(files.value)
  directory.value = '/usr'
  files.value = [{ id: 'bin', isSystem: false }] as IFile[]
  browsing.value = true
  await nextTick()
  expect(group.expanded.value).toBe(false)
  expect(group.systemItems.value).toEqual([])
  expect(group.visibleItems.value.map(item => item.id)).toEqual(['bin'])
  scope.stop()
})
