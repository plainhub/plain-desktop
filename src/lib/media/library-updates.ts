import { graphqlSubscriptions } from '@/lib/api/subscription-client'
import { createRefreshScheduler } from '@/lib/refresh-scheduler'
import { DataType } from '@/lib/data'

export function subscribeMediaLibraryUpdates(type: DataType, refresh: () => void): () => void {
  const scheduler = createRefreshScheduler(refresh)
  const stop = graphqlSubscriptions.subscribe('mediaUpdated', ({ types }) => {
    if (types.includes(type)) scheduler.schedule()
  })
  return () => { stop(); scheduler.dispose() }
}
