import { graphqlSubscriptions } from '@/lib/api/subscription-client'
import { createRefreshScheduler } from '@/lib/refresh-scheduler'
import { DataType } from '@/lib/data'

const kinds = new Map<DataType, number>([[DataType.AUDIO, 1], [DataType.VIDEO, 2], [DataType.IMAGE, 3], [DataType.DOC, 24]])
export function subscribeMediaLibraryUpdates(type: DataType, refresh: () => void): () => void {
  const kind = kinds.get(type)
  const scheduler = createRefreshScheduler(refresh)
  const stop = graphqlSubscriptions.subscribe('mediaLibraryUpdated', ({ types }) => {
    if (kind !== undefined && types.includes(kind)) scheduler.schedule()
  })
  return () => { stop(); scheduler.dispose() }
}
