import { gqlFetch } from '@/lib/api/gql-client'
import { arrayBufferToBase64, base64ToArrayBuffer } from '@/lib/strutil'
import type { ProjectData, ProjectMeta, ProjectStore, ProjectSummary } from './project-store'

interface GqlProjectSummary {
  id: string
  thumbnail: string | null
  canvasWidth: number
  canvasHeight: number
  updatedAt: string
}

interface GqlProject extends GqlProjectSummary {
  stateB64: string
}

const LIST_QUERY = `
  query {
    imageEditorProjects {
      id
      thumbnail
      canvasWidth
      canvasHeight
      updatedAt
    }
  }
`

const GET_QUERY = `
  query imageEditorProject($id: ID!) {
    imageEditorProject(id: $id) {
      id
      stateB64
      thumbnail
      canvasWidth
      canvasHeight
      updatedAt
    }
  }
`

const SAVE_MUTATION = `
  mutation saveImageEditorProject($id: ID!, $input: ImageEditorProjectInput!) {
    saveImageEditorProject(id: $id, input: $input) {
      id
    }
  }
`

const DELETE_MUTATION = `
  mutation deleteImageEditorProject($id: ID!) {
    deleteImageEditorProject(id: $id)
  }
`

function parseUpdatedAt(value: string): number {
  const ms = Date.parse(value)
  return Number.isNaN(ms) ? Date.now() : ms
}

function toSummary(p: GqlProjectSummary): ProjectSummary {
  return {
    id: p.id,
    updatedAt: parseUpdatedAt(p.updatedAt),
    canvasWidth: p.canvasWidth,
    canvasHeight: p.canvasHeight,
    previewDataUrl: p.thumbnail,
  }
}

export class PlainAppProjectStore implements ProjectStore {
  async save(id: string, data: ProjectData, meta: ProjectMeta): Promise<void> {
    const stateB64 = arrayBufferToBase64(data.state.buffer)
    const input = {
      stateB64,
      thumbnail: data.thumbnail,
      canvasWidth: meta.canvasWidth,
      canvasHeight: meta.canvasHeight,
    }
    const r = await gqlFetch(SAVE_MUTATION, { id, input })
    if (r.errors?.length) throw new Error(r.errors[0]!.message)
  }

  async load(id: string): Promise<ProjectData | null> {
    const r = await gqlFetch<{ imageEditorProject: GqlProject | null }>(GET_QUERY, { id })
    if (r.errors?.length) throw new Error(r.errors[0]!.message)
    const project = r.data?.imageEditorProject
    if (!project) return null
    return {
      state: new Uint8Array(base64ToArrayBuffer(project.stateB64)),
      thumbnail: project.thumbnail,
    }
  }

  async delete(id: string): Promise<void> {
    const r = await gqlFetch(DELETE_MUTATION, { id })
    if (r.errors?.length) throw new Error(r.errors[0]!.message)
  }

  async list(): Promise<ProjectSummary[]> {
    const r = await gqlFetch<{ imageEditorProjects: GqlProjectSummary[] }>(LIST_QUERY)
    if (r.errors?.length) return []
    return (r.data?.imageEditorProjects ?? []).map(toSummary)
  }
}
