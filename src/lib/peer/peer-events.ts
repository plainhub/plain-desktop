import emitter from '@/plugins/eventbus'

export function projectPeerNotificationEvent(value: unknown, native: boolean): void {
  if (!native || !value || typeof value !== 'object') return
  const update = value as import('./local-peer-data').PeerNotificationUpdate
  if (!Number.isSafeInteger(update.revision) || update.revision < 0) return
  emitter.emit('peer_notifications_updated', update)
}
