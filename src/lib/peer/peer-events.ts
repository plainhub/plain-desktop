import emitter from '@/plugins/eventbus'
import { findLoginPeer } from '@/lib/device/login-peers'

export function projectPeerManagementEvent(value: unknown, native: boolean): void {
  if (!native || !value || typeof value !== 'object') return
  const event = value as { peerId?: unknown; type?: unknown; data?: unknown }
  if (typeof event.peerId !== 'string' || typeof event.type !== 'string' || !event.data || !findLoginPeer(event.peerId)) return
  emitter.emit('peer_ws_event', { peerId: event.peerId, type: event.type, data: event.data })
}
