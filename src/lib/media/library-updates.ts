import { graphqlSubscriptions } from '@/lib/api/subscription-client'
import { createRefreshScheduler } from '@/lib/refresh-scheduler'
import type { SubscriptionResults } from '@/lib/api/subscription-results'

export type MediaDataType = SubscriptionResults['mediaUpdated']['types'][number]

export function subscribeMediaLibraryUpdates(type: MediaDataType, refresh: () => void): () => void {
  const scheduler = createRefreshScheduler(refresh)
  const stop = graphqlSubscriptions.subscribe('mediaUpdated', ({ types }) => {
    if (types.includes(type)) scheduler.schedule()
  })
  return () => { stop(); scheduler.dispose() }
}
