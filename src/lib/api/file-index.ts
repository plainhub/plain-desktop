import { GqlError } from '@/lib/api/gql-error'
import { bindGraphqlClient } from './gql-client'

export type FileIndexMode = 'UNCONFIGURED' | 'DISABLED' | 'ALLOWLIST' | 'BLOCKLIST'

export type FileIndexScanStatus = 'COMPLETED' | 'CANCELED' | 'FAILED'

export interface FileIndexVolume {
  id: string
  path: string
  isAvailable: boolean
  indexMode: FileIndexMode
  includedDirs: string[]
  excludedDirs: string[]
  lastScanAt: string | null
  lastScanStatus: FileIndexScanStatus | null
  lastScanCount: number | null
  lastScanTotal: number | null
}

export interface FileIndexDefaults {
  excludedDirs: string[]
  excludedNames: string[]
  excludedProfileDirs: FileIndexProfileDir[]
  mediaExcludedDirSuffixes: string[]
}

export interface FileIndexProfileDir {
  profilesDir: string
  excludedDir: string
  exceptDirs: string[]
}

export const fileIndexVolumesGQL = `query {
  fileIndexVolumes { id path isAvailable indexMode includedDirs excludedDirs lastScanAt lastScanStatus lastScanCount lastScanTotal }
  fileIndexDefaults { excludedDirs excludedNames excludedProfileDirs { profilesDir excludedDir exceptDirs } mediaExcludedDirSuffixes }
}`

const setFileIndexVolumeGQL = `mutation($volumeId: ID!, $input: FileIndexVolumeInput!) {
  setFileIndexVolume(volumeId: $volumeId, input: $input)
}`

export async function saveFileIndexVolume(volume: FileIndexVolume, request = bindGraphqlClient()): Promise<void> {
  const { id, indexMode, includedDirs, excludedDirs } = volume
  const result = await request<{ setFileIndexVolume: boolean }>(setFileIndexVolumeGQL, { volumeId: id, input: { indexMode, includedDirs, excludedDirs } })
  if (result.errors?.length) throw GqlError.fromGraphql(result.errors[0])
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
  if (result.errors?.length) throw GqlError.fromGraphql(result.errors[0])
  if (!result.data.rebuildFileIndex) throw new Error('File index rebuild refused')
}
