import { afterEach, expect, it, vi } from 'vitest'
import { gqlFetch } from '@/lib/api/gql-client'
import { PlainAppProjectStore } from '@/views/image-editor/store/plain-app-store'

vi.mock('@/lib/api/gql-client', () => ({ gqlFetch: vi.fn() }))

afterEach(() => vi.resetAllMocks())

it('saves, lists and restores a project without layer count metadata', async () => {
  const project = {
    id: 'project',
    stateB64: 'AQID',
    thumbnail: null,
    canvasWidth: 320,
    canvasHeight: 240,
    updatedAt: '2026-01-01T00:00:00Z',
  }
  const fetch = vi.mocked(gqlFetch)
  fetch.mockResolvedValueOnce({ data: { saveImageEditorProject: { id: project.id } } })
    .mockResolvedValueOnce({ data: { imageEditorProjects: [project] } })
    .mockResolvedValueOnce({ data: { imageEditorProject: project } })

  const store = new PlainAppProjectStore()
  const state = new Uint8Array([1, 2, 3])
  await store.save(project.id, { state, thumbnail: null }, { canvasWidth: 320, canvasHeight: 240 })

  expect(fetch.mock.calls[0]?.[1]).toStrictEqual({
    id: project.id,
    input: { stateB64: 'AQID', thumbnail: null, canvasWidth: 320, canvasHeight: 240 },
  })
  expect(await store.list()).toEqual([{
    id: project.id,
    updatedAt: Date.parse(project.updatedAt),
    canvasWidth: 320,
    canvasHeight: 240,
    previewDataUrl: null,
  }])
  expect(await store.load(project.id)).toEqual({ state, thumbnail: null })
  for (const [query] of fetch.mock.calls) expect(query).not.toContain('layerCount')
})
