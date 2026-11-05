import { ref, type Ref } from 'vue'
import type { ImageEditorDoc } from './useImageEditorDoc'
import type { ProjectStore, ProjectSummary } from '../store/project-store'
import { generateRecordId } from '@/lib/record-id.generated'

export type { ProjectSummary }

const SAVE_DEBOUNCE_MS = 800
const THUMBNAIL_DEBOUNCE_MS = 5000
const EDITOR_BASE = '/image-editor'

export function useImageEditorPersistence(
  doc: ImageEditorDoc,
  store: ProjectStore,
  editorActive: Ref<boolean>,
  generateThumbnail: () => string | null,
  onRestored: () => void,
) {
  const projectId = ref<string | null>(null)
  let saveTimer: ReturnType<typeof setTimeout> | null = null
  let thumbnailTimer: ReturnType<typeof setTimeout> | null = null
  let cachedThumbnail: string | null = null

  function getIdFromUrl(): string | null {
    if (typeof window === 'undefined') return null
    const match = window.location.pathname.match(/\/image-editor\/([A-Za-z0-9]+)$/)
    if (match?.[1] === 'new') return null
    return match?.[1] ?? null
  }

  function setIdInUrl(id: string) {
    if (typeof window === 'undefined') return
    const target = `${EDITOR_BASE}/${id}`
    if (!window.location.pathname.endsWith(`/${id}`)) {
      window.history.replaceState({}, '', target)
    }
  }

  function ensureProjectId(): string {
    if (projectId.value) return projectId.value
    const urlId = getIdFromUrl()
    if (urlId) { projectId.value = urlId; return urlId }
    const newId = generateRecordId()
    projectId.value = newId
    setIdInUrl(newId)
    return newId
  }

  function scheduleSave() {
    if (typeof window === 'undefined') return
    if (!editorActive.value) return
    if (thumbnailTimer) clearTimeout(thumbnailTimer)
    thumbnailTimer = setTimeout(() => {
      cachedThumbnail = generateThumbnail()
    }, THUMBNAIL_DEBOUNCE_MS)
    if (saveTimer) clearTimeout(saveTimer)
    saveTimer = setTimeout(() => doSave(), SAVE_DEBOUNCE_MS)
  }

  async function doSave(): Promise<void> {
    if (!editorActive.value) return
    const id = ensureProjectId()
    try {
      const state = doc.getStateUpdate()
      const { width, height } = doc.getCanvasSize()
      await store.save(
        id,
        { state, thumbnail: cachedThumbnail },
        { canvasWidth: width, canvasHeight: height, layerCount: doc.getLayerCount() },
      )
    } catch (e) {
      console.warn('[ImageEditor] Failed to save project', e)
    }
  }

  async function flushSave(): Promise<void> {
    if (saveTimer) { clearTimeout(saveTimer); saveTimer = null }
    if (thumbnailTimer) {
      clearTimeout(thumbnailTimer)
      thumbnailTimer = null
      cachedThumbnail = generateThumbnail()
    }
    await doSave()
  }

  async function tryRestore(): Promise<boolean> {
    if (typeof window === 'undefined') return false
    const urlId = getIdFromUrl()
    if (!urlId) return false
    try {
      const data = await store.load(urlId)
      if (!data) return false
      projectId.value = urlId
      cachedThumbnail = data.thumbnail
      doc.loadFromStateV2(data.state)
      editorActive.value = true
      onRestored()
      return true
    } catch (e) {
      console.warn('[ImageEditor] Failed to restore project', e)
      return false
    }
  }

  function clearProject() {
    if (saveTimer) clearTimeout(saveTimer)
    if (thumbnailTimer) clearTimeout(thumbnailTimer)
    cachedThumbnail = null
    const id = projectId.value
    if (id) store.delete(id).catch(() => {})
    projectId.value = null
    if (typeof window !== 'undefined') {
      window.history.replaceState({}, '', EDITOR_BASE)
    }
  }

  async function deleteProject(): Promise<void> {
    if (saveTimer) clearTimeout(saveTimer)
    if (thumbnailTimer) clearTimeout(thumbnailTimer)
    const id = projectId.value
    projectId.value = null
    if (id) {
      await store.delete(id).catch(() => {})
    }
    if (typeof window !== 'undefined') {
      window.close()
      window.location.href = EDITOR_BASE
    }
  }

  async function loadProjectById(id: string): Promise<boolean> {
    if (typeof window === 'undefined') return false
    try {
      const data = await store.load(id)
      if (!data) return false
      projectId.value = id
      setIdInUrl(id)
      doc.loadFromStateV2(data.state)
      editorActive.value = true
      onRestored()
      return true
    } catch {
      return false
    }
  }

  return {
    projectId,
    scheduleSave,
    flushSave,
    tryRestore,
    ensureProjectId,
    clearProject,
    deleteProject,
    listRecentProjects: () => store.list(),
    loadProjectById,
  }
}
