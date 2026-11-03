import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { createRenderer, nextTick, ref } from 'vue'
const io = vi.hoisted(() => ({ route: { params: { id: 'create' } }, fetch: vi.fn() }))
vi.mock('vue-router', () => ({ useRoute: () => io.route }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('pinia', () => ({ storeToRefs: (value: unknown) => value }))
vi.mock('@/stores/main', () => ({ useMainStore: () => ({}) }))
vi.mock('@/stores/temp', () => ({ useTempStore: () => ({ app: ref({}), urlTokenKey: ref(null) }) }))
vi.mock('@/components/toaster', () => ({ default: vi.fn() }))
vi.mock('@/lib/api/query', () => ({ noteGQL: 'note-query', tagsGQL: 'tags-query', initLazyQuery: () => ({ fetch: vi.fn() }), initQuery: () => ({ refetch: vi.fn() }) }))
vi.mock('@/hooks/markdown', () => ({ useMarkdown: () => ({ render: async (value: string) => value }) }))
vi.mock('@/plugins/router', () => ({ default: { currentRoute: ref({ query: {} }) }, replacePath: vi.fn(), replacePathNoReload: vi.fn() }))
vi.mock('@/plugins/eventbus', () => ({ default: { on: vi.fn(), off: vi.fn(), emit: vi.fn() } }))
vi.mock('@/lib/upload/upload', () => ({ upload: vi.fn() }))
vi.mock('@/lib/api/gql-client', () => ({ gqlFetch: io.fetch, GqlError: class extends Error {} }))
import { useNoteEdit } from '@/hooks/note-edit'
beforeEach(() => {
  vi.useFakeTimers()
  io.fetch.mockReset().mockImplementation(async (document: string, variables: { id?: string; input: { title: string; content: string } }) => {
    const creating = document.includes('mutation createNote')
    return { data: { [creating ? 'createNote' : 'updateNote']: { id: creating ? 'created-note' : variables.id, ...variables.input, tags: [], updatedAt: '2000-01-01T00:00:00Z' } } }
  })
})
afterEach(() => vi.useRealTimers())
function mountEdit(initial: string) {
  io.route.params.id = initial
  let edit!: ReturnType<typeof useNoteEdit>
  const renderer = createRenderer<any, any>({ insert: vi.fn(), remove: vi.fn(), createElement: () => ({}), createText: () => ({}), createComment: () => ({}), setText: vi.fn(), setElementText: vi.fn(), parentNode: () => null, nextSibling: () => null, patchProp: vi.fn() })
  const app = renderer.createApp({ setup() { edit = useNoteEdit(); return () => null } }); app.mount({})
  return { edit, app }
}

it.each(['create', 'existing-note'])('chooses the actual note operation at save time for %s', async initial => {
  const { edit, app } = mountEdit(initial)
  try {
    edit.content.value = '# First 中文\n\nOriginal body.\n'
    await nextTick(); await vi.advanceTimersByTimeAsync(500)
    expect(io.fetch).toHaveBeenCalledTimes(1)
    expect(io.fetch.mock.calls[0]![0]).toContain(initial === 'create' ? 'mutation createNote' : 'mutation updateNote')
    const id = initial === 'create' ? 'created-note' : initial
    expect(edit.id.value).toBe(id)
    edit.content.value += '\nSecond edit retained.\n'
    await nextTick(); await vi.advanceTimersByTimeAsync(500)
    expect(io.fetch).toHaveBeenCalledTimes(2)
    expect(io.fetch.mock.calls[1]![0]).toContain('mutation updateNote')
    expect(io.fetch.mock.calls[1]![1]).toMatchObject({ id, input: { content: edit.content.value } })
    expect(edit.note.value?.id).toBe(id)
    expect(edit.note.value?.content).toBe(edit.content.value)
  } finally { app.unmount() }
})

it('serializes a delayed creation and coalesces later edits into one update', async () => {
  let finish!: (value: unknown) => void
  io.fetch.mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))
  const { edit, app } = mountEdit('create')
  const first = '# First\n\nInitial.\n'
  const latest = '# First\n\nLatest retained 中文.\n'
  try {
    edit.content.value = first
    await nextTick(); await vi.advanceTimersByTimeAsync(500)
    expect(io.fetch).toHaveBeenCalledTimes(1)
    edit.content.value = first + 'Intermediate edit.\n'
    await nextTick(); await vi.advanceTimersByTimeAsync(500)
    edit.content.value = latest
    await nextTick(); await vi.advanceTimersByTimeAsync(500)
    expect(io.fetch).toHaveBeenCalledTimes(1)
    expect(edit.notSaved.value).toBe(true)
    finish({ data: { createNote: { id: 'created-note', title: 'First', content: first, tags: [] } } })
    await vi.advanceTimersByTimeAsync(0)
    expect(io.fetch).toHaveBeenCalledTimes(2)
    expect(io.fetch.mock.calls[1]![0]).toContain('mutation updateNote')
    expect(io.fetch.mock.calls[1]![1]).toMatchObject({ id: 'created-note', input: { content: latest } })
    expect(edit.note.value?.content).toBe(latest)
    expect(edit.notSaved.value).toBe(false)
  } finally { finish?.({ data: { createNote: { id: 'created-note', content: first } } }); app.unmount() }
})
it.each(['create', 'existing-note'])('retains unsaved state after an API error and accepts a later edit for %s', async initial => {
  io.fetch.mockResolvedValueOnce({ errors: [{ message: 'Permission denied' }] })
  const { edit, app } = mountEdit(initial)
  try {
    edit.content.value = '# First\n\nUnsaved body.\n'
    await nextTick(); await vi.advanceTimersByTimeAsync(500)
    expect(io.fetch).toHaveBeenCalledOnce()
    expect(edit.notSaved.value).toBe(true)
    expect(edit.id.value).toBe(initial === 'create' ? '' : initial)
    edit.content.value += '\nLater edit.\n'
    await nextTick(); await vi.advanceTimersByTimeAsync(500)
    expect(io.fetch).toHaveBeenCalledTimes(2)
    expect(edit.notSaved.value).toBe(false)
    expect(edit.note.value?.content).toBe(edit.content.value)
  } finally { app.unmount() }
})
