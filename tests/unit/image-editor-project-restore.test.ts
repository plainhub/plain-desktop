import { afterEach, expect, it, vi } from 'vitest'
import { ref } from 'vue'
import { useImageEditorPersistence } from '@/views/image-editor/composables/useImageEditorPersistence'
import { useImageEditorDoc } from '@/views/image-editor/composables/useImageEditorDoc'

afterEach(() => vi.unstubAllGlobals())

it('restores a saved project with the generated UUID format instead of creating an empty project', async () => {
  const id = '019a891e-1470-7c61-8402-2dfe660140cc'
  const saved = useImageEditorDoc()
  saved.setCanvasSize(320, 240)
  saved.setBgColor('#22c55e')
  const store = { load: vi.fn(async () => ({ state: saved.getStateUpdate(), thumbnail: null })), save: vi.fn(), list: vi.fn(), delete: vi.fn() }
  const history = { replaceState: vi.fn() }
  vi.stubGlobal('window', { location: { pathname: '/image-editor/' + id }, history })
  const doc = useImageEditorDoc()
  const active = ref(false)
  const restored = vi.fn()
  const persistence = useImageEditorPersistence(doc, store, active, () => null, restored)
  expect(await persistence.tryRestore()).toBe(true)
  expect(store.load).toHaveBeenCalledExactlyOnceWith(id)
  expect(doc.getCanvasSize()).toEqual({ width: 320, height: 240 })
  expect(doc.getBgColor()).toBe('#22c55e')
  expect(active.value).toBe(true)
  expect(restored).toHaveBeenCalledOnce()
  expect(persistence.ensureProjectId()).toBe(id)
  expect(history.replaceState).not.toHaveBeenCalled()
  saved.ydoc.destroy(); doc.ydoc.destroy()
})

it('reports a failed saved-project read instead of silently treating it as a new document', async () => {
  vi.stubGlobal('window', { location: { pathname: '/image-editor/019a891e-1470-7c61-8402-2dfe660140cc' } })
  const failure = new Error('Read failed')
  const store = { load: vi.fn().mockRejectedValue(failure), save: vi.fn(), list: vi.fn(), delete: vi.fn() }
  const doc = useImageEditorDoc()
  const active = ref(false)
  const persistence = useImageEditorPersistence(doc, store, active, () => null, vi.fn())
  await expect(persistence.tryRestore()).rejects.toBe(failure)
  expect(active.value).toBe(false)
  expect(store.save).not.toHaveBeenCalled()
  doc.destroy()
})

it('keeps a project open when deletion fails and stops autosaving after successful deletion', async () => {
  const id = '019a891e-1470-7c61-8402-2dfe660140cc'
  const close = vi.fn()
  vi.stubGlobal('window', { location: { pathname: '/image-editor/' + id, href: '' }, close })
  const doc = useImageEditorDoc()
  const store = { load: vi.fn(async () => ({ state: doc.getStateUpdate(), thumbnail: null })), save: vi.fn(), list: vi.fn(), delete: vi.fn().mockRejectedValue(new Error('Delete failed')) }
  const active = ref(false)
  const persistence = useImageEditorPersistence(doc, store, active, () => null, vi.fn())
  await persistence.tryRestore()
  await expect(persistence.deleteProject()).rejects.toThrow('Delete failed')
  expect(close).not.toHaveBeenCalled()
  expect(active.value).toBe(true)
  expect(persistence.ensureProjectId()).toBe(id)
  store.delete.mockResolvedValue(undefined)
  await persistence.deleteProject()
  await persistence.flushSave()
  expect(active.value).toBe(false)
  expect(store.save).not.toHaveBeenCalled()
  expect(close).toHaveBeenCalledOnce()
  doc.destroy()
})
