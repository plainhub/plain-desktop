import { graphqlSubscriptions } from '@/lib/api/subscription-client'
import { DataType } from '@/lib/data'

const kinds = new Map<DataType, number>([[DataType.AUDIO, 1], [DataType.VIDEO, 2], [DataType.IMAGE, 3], [DataType.DOC, 24]])
export function subscribeMediaLibraryUpdates(type: DataType, refresh: () => void): () => void {
  const kind = kinds.get(type)
  return graphqlSubscriptions.subscribe('mediaLibraryUpdated', ({ types }) => {
    if (kind !== undefined && types.includes(kind)) refresh()
  })
}
