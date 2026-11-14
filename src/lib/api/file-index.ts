import { bindGraphqlClient } from './gql-client'

export type FileIndexMode = 'UNCONFIGURED' | 'DISABLED' | 'ALLOWLIST' | 'BLOCKLIST'

export interface FileIndexVolume {
  id: string
  path: string
  isAvailable: boolean
  indexMode: FileIndexMode
  includedDirs: string[]
  excludedDirs: string[]
}

export interface FileIndexDefaults {
  excludedDirs: string[]
  excludedNames: string[]
  excludedPackages: string[]
  excludedUserLibraries: boolean
}

export const fileIndexVolumesGQL = `query {
  fileIndexVolumes { id path isAvailable indexMode includedDirs excludedDirs }
  fileIndexDefaults { excludedDirs excludedNames excludedPackages excludedUserLibraries }
}`

const setFileIndexVolumeGQL = `mutation($id: ID!, $indexMode: FileIndexMode!, $includedDirs: [String!]!, $excludedDirs: [String!]!) {
  setFileIndexVolume(id: $id, indexMode: $indexMode, includedDirs: $includedDirs, excludedDirs: $excludedDirs)
}`

export async function saveFileIndexVolume(volume: FileIndexVolume, request = bindGraphqlClient()): Promise<void> {
  const { id, indexMode, includedDirs, excludedDirs } = volume
  const result = await request<{ setFileIndexVolume: boolean }>(setFileIndexVolumeGQL, { id, indexMode, includedDirs, excludedDirs })
  if (result.errors?.length) throw new Error(result.errors[0].message)
  if (!result.data.setFileIndexVolume) throw new Error('Index configuration write refused')
}

export const rebuildFileIndexGQL = `mutation($volumeId: ID) {
  rebuildFileIndex(volumeId: $volumeId)
}`

export function canScanVolume(volume: FileIndexVolume): boolean {
  return volume.isAvailable && (volume.indexMode === 'BLOCKLIST' || (volume.indexMode === 'ALLOWLIST' && volume.includedDirs.length > 0))
}

export async function rebuildFileIndex(volumeId: string | null, request = bindGraphqlClient()): Promise<void> {
  const result = await request<{ rebuildFileIndex: boolean }>(rebuildFileIndexGQL, { volumeId })
  if (result.errors?.length) throw new Error(result.errors[0].message)
  if (!result.data.rebuildFileIndex) throw new Error('File index rebuild refused')
}
