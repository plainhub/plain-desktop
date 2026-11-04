import { beforeEach, expect, it, vi } from 'vitest'
import { ref } from 'vue'
import type { IFile } from '@/lib/file'

vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('@/components/toaster', () => ({ default: vi.fn() }))
vi.mock('@/lib/api/query', () => ({ initQuery: vi.fn(), favoriteFoldersGQL: '', mountsGQL: '' }))
vi.mock('@/lib/api/file', () => ({ getFileName: (path: string) => path.split('/').at(-1) }))
vi.mock('@/lib/api/api', () => ({ getApiBaseUrl: vi.fn() }))
vi.mock('@/plugins/eventbus', () => ({ default: { emit: vi.fn() } }))
vi.mock('@/lib/api/gql-client', () => ({ gqlFetch: vi.fn(), GqlError: class extends Error {} }))
import { gqlFetch } from '@/lib/api/gql-client'
import toast from '@/components/toaster'
import { useCopyPaste, useView, useRename } from '@/hooks/files'
import type { ISource } from '@/components/lightbox/types'

const sources = [{ id: 'one', path: '/source/one', name: 'one' }, { id: 'two', path: '/source/two', name: 'two' }] as IFile[]
function fixture(move = false) {
  const items = ref([...sources])
  const selection = ref<IFile[]>([])
  const refreshFiles = vi.fn()
  const refreshStats = vi.fn()
  const view = useCopyPaste(items, ref(false), selection, refreshFiles, refreshStats)
  if (move) view.cut(sources.map(file => file.id))
  else view.copy(sources.map(file => file.id))
  items.value = [{ id: 'neighbor', path: '/destination/neighbor', name: 'neighbor' } as IFile]
  return { view, items, selection, refreshFiles, refreshStats }
}
beforeEach(() => { vi.mocked(gqlFetch).mockReset(); vi.mocked(toast).mockReset() })

it.each([false, true])('submits the whole selection once and waits for %s completion before clearing it', async move => {
  const f = fixture(move)
  let finish!: (value: any) => void
  vi.mocked(gqlFetch).mockImplementation(() => new Promise(resolve => { finish = resolve }))
  const completed = f.view.paste('/destination')
  try {
    expect(gqlFetch).toHaveBeenCalledTimes(1)
    const [document, variables] = vi.mocked(gqlFetch).mock.calls[0]!
    expect(document).toContain('createFileTask')
    expect(document).toMatch(/waitForCompletion:\s*true/)
    expect(variables).toEqual({ type: move ? 'MOVE' : 'COPY', title: move ? 'MOVE' : 'COPY', ops: sources.map(file => ({ src: file.path, dst: '/destination/' + file.name, overwrite: false })) })
    expect(f.view.loading.value).toBe(true)
    expect(f.selection.value).toHaveLength(2)
    expect(f.refreshFiles).not.toHaveBeenCalled()
  } finally { finish({ data: { createFileTask: { status: 'DONE', error: '' } } }); await completed }
  expect(f.selection.value).toEqual([])
  expect(f.items.value.map(file => file.id)).toEqual(['neighbor'])
  expect(f.view.loading.value).toBe(false)
  expect(f.refreshFiles).toHaveBeenCalledTimes(1)
  expect(f.refreshStats).toHaveBeenCalledTimes(1)
})

it.each(['task', 'graphql', 'transport'])('retains the selection and refreshes actual state on %s failure', async mode => {
  const f = fixture(true)
  if (mode === 'transport') vi.mocked(gqlFetch).mockRejectedValue(new Error('connection lost'))
  else vi.mocked(gqlFetch).mockResolvedValue(mode === 'task' ? { data: { createFileTask: { status: 'ERROR', error: 'write refused' } } } : { errors: [{ message: 'missing source' }] })
  await f.view.paste('/destination')
  expect(f.selection.value.map(file => file.id)).toEqual(['one', 'two'])
  expect(f.items.value.map(file => file.id)).toEqual(['neighbor'])
  expect(f.refreshFiles).toHaveBeenCalledTimes(1)
  expect(f.refreshStats).toHaveBeenCalledTimes(1)
  if (mode === 'task') expect(toast).toHaveBeenCalledExactlyOnceWith('write refused')
})

it('leaves a newer clipboard selection intact when an earlier paste completes', async () => {
  const f = fixture()
  let finish!: (value: any) => void
  vi.mocked(gqlFetch).mockImplementation(() => new Promise(resolve => { finish = resolve }))
  const completed = f.view.paste('/destination')
  f.view.copy(['neighbor'])
  finish({ data: { createFileTask: { status: 'DONE', error: '' } } })
  await completed
  expect(f.selection.value.map(file => file.id)).toEqual(['neighbor'])
})

it('keeps an empty clipboard as a no-op', async () => {
  const f = fixture()
  f.selection.value = []
  await f.view.paste('/destination')
  expect(gqlFetch).not.toHaveBeenCalled()
  expect(f.refreshFiles).not.toHaveBeenCalled()
})


it('keeps each preview source attached to its own file and modification timestamp', () => {
  const files = [
    { id: 'selected', name: 'selected.wav', path: '/selected.wav', isDir: false, size: 16044, updatedAt: '2026-01-01T00:00:00.001Z' },
    { id: 'neighbor', name: 'neighbor.wav', path: '/neighbor.wav', isDir: false, size: 33, updatedAt: '2026-01-01T00:00:00.002Z' },
  ] as IFile[]
  const previews = ref<ISource[]>([])
  const open = vi.fn()
  useView(previews, open).view(files, files[0]!)
  expect(previews.value.map(item => ({ path: item.path, updatedAt: item.updatedAt, itemPath: item.data.path }))).toEqual(
    files.map(item => ({ path: item.path, updatedAt: item.updatedAt, itemPath: item.path }))
  )
  expect(open).toHaveBeenCalledWith(previews.value, 0)
})

it.each([null, '/source/renamed.txt'])('accepts successful rename paths and rejects null results (%s)', async result => {
  const refresh = vi.fn()
  const view = useRename(refresh)
  vi.mocked(gqlFetch).mockResolvedValue({ data: { renameFile: result } })
  await view.renameMutation().mutate({ path: '/source/one', name: 'renamed.txt' })
  expect(view.renameDone('renamed.txt')).toBe(result !== null)
  if (result) {
    expect(refresh).toHaveBeenCalledOnce()
    expect(toast).not.toHaveBeenCalled()
  } else {
    expect(refresh).not.toHaveBeenCalled()
    expect(toast).toHaveBeenCalledExactlyOnceWith('rename_failed', 'error')
  }
})
