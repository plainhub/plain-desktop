export type UpdateHandler = (update: Uint8Array) => void
export type Unsubscribe = () => void

export interface SyncTransport {
  readonly connected: boolean
  connect(): Promise<void>
  disconnect(): void
  broadcastUpdate(update: Uint8Array): void
  onUpdate(handler: UpdateHandler): Unsubscribe
  destroy(): void
}
