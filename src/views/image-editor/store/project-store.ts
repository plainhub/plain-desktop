/**
 * Persistence abstraction for image-editor projects.
 *
 * Stores a Yjs binary state update (the CRDT document) plus lightweight
 * metadata so the list view can render summaries without decoding the full
 * document.
 *
 * `state` is always `Y.encodeStateAsUpdate(ydoc)` — a self-contained binary
 * snapshot that can be re-applied via `Y.applyUpdate(ydoc, state)`.
 */

export interface ProjectMeta {
  canvasWidth: number
  canvasHeight: number
}

export interface ProjectSummary {
  id: string
  updatedAt: number
  canvasWidth: number
  canvasHeight: number
  previewDataUrl: string | null
}

export interface ProjectData {
  state: Uint8Array
  thumbnail: string | null
}

export interface ProjectStore {
  save(id: string, data: ProjectData, meta: ProjectMeta): Promise<void>
  load(id: string): Promise<ProjectData | null>
  delete(id: string): Promise<void>
  list(): Promise<ProjectSummary[]>
}
