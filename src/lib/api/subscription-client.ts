import { isNativeTarget } from './api'
import { openSubscriptionConnection, type SubscriptionConnection } from './subscription-connection'
import type { SubscriptionOperation } from './subscription-operations'
import type { SubscriptionField, SubscriptionResults } from './subscription-results'

interface Message {
  type: 'next' | 'error' | 'complete'
  id: string
  payload?: { data?: Record<string, unknown>; errors?: { message: string }[]; extensions?: { subscriptionReady?: boolean } } | { message: string }[]
}
export class SubscriptionClient {
  private connection: SubscriptionConnection | undefined
  private operations = new Map<string, SubscriptionOperation>()
  private ids = new Map<string, string>()
  private sequence = 0
  private pending = new Set<string>()
  private acknowledged = false
  private active = false
  private retry: ReturnType<typeof setTimeout> | undefined
  private failures = 0
  private generation = 0
  private consumers = new Map<SubscriptionField, Set<(data: unknown) => void | Promise<unknown>>>()
  private available: readonly SubscriptionOperation[] = []
  private attachments = new Set<() => void>()
  private connectionListeners = new Set<(connected: boolean | null) => void>()
  onConnectionChange(listener: (connected: boolean | null) => void): () => void {
    this.connectionListeners.add(listener)
    return () => { this.connectionListeners.delete(listener) }
  }
  subscribe<K extends SubscriptionField>(field: K, next: (data: SubscriptionResults[K]) => void | Promise<unknown>): () => void {
    const consumers = this.consumers.get(field) ?? new Set<(data: unknown) => void | Promise<unknown>>()
    this.consumers.set(field, consumers)
    const consume = (data: unknown) => next(data as SubscriptionResults[K])
    consumers.add(consume)
    this.updateOperations()
    return () => {
      if (!consumers.delete(consume)) return
      if (!consumers.size) this.consumers.delete(field)
      this.updateOperations()
    }
  }
  onAttached(next: () => void): () => void {
    const attached = () => next()
    this.attachments.add(attached)
    return () => { this.attachments.delete(attached) }
  }
  private ready() {
    for (const next of [...this.attachments]) {
      if (!this.attachments.has(next)) continue
      try { next() } catch (error) { console.error('GraphQL subscription snapshot failed', error) }
    }
  }
  private deliver(operation: SubscriptionOperation, data: unknown) {
    const consumers = this.consumers.get(operation.field)
    if (!consumers) return
    for (const next of [...consumers]) {
      if (!consumers.has(next)) continue
      try {
        const result = next(data)
        if (result) void result.catch(error => console.error('GraphQL subscription consumer failed', operation.field, error))
      } catch (error) { console.error('GraphQL subscription consumer failed', operation.field, error) }
    }
  }
  start() { this.active = true; this.connect() }
  setOperations(operations: readonly SubscriptionOperation[]) {
    this.available = operations
    this.updateOperations()
  }
  private updateOperations() {
    const desired = new Map<string, SubscriptionOperation>(this.available.filter(operation => this.consumers.has(operation.field)).map(operation => [operation.field, operation]))
    for (const id of this.operations.keys()) {
      if (!desired.has(id)) {
        for (const [operationId, field] of this.ids) {
          if (field !== id) continue
          if (this.acknowledged) this.connection?.send({ id: operationId, type: 'complete' })
          this.ids.delete(operationId)
          this.pending.delete(operationId)
        }
      }
    }
    for (const [id, operation] of desired) if (!this.operations.has(id) && this.acknowledged) this.send(operation)
    this.operations = desired
  }
  private send(operation: SubscriptionOperation) {
    const id = `${this.generation}:${++this.sequence}`
    this.ids.set(id, operation.field)
    this.pending.add(id)
    this.connection?.send({ type: 'subscribe', id, payload: { query: operation.document } })
  }
  private connect() {
    if (!this.active) return
    const generation = ++this.generation
    try {
      this.connection = openSubscriptionConnection({
        opened: () => {
          if (generation !== this.generation) return
          this.acknowledged = true
          for (const listener of this.connectionListeners) listener(true)
          this.failures = 0
          this.pending.clear()
          this.ids.clear()
          for (const operation of this.operations.values()) this.send(operation)
        },
        message: message => { if (generation === this.generation) this.receive(message) },
        dropped: error => { if (generation === this.generation) this.dropped(error) },
      })
    } catch (error) { this.dropped(error) }
  }
  private receive(value: unknown) {
    if (!value || typeof value !== 'object' || !('type' in value) || !('id' in value) || typeof value.id !== 'string') throw new Error('Invalid subscription message')
    const message = value as Message
    const operation = this.operations.get(this.ids.get(message.id) ?? '')
    if (!operation) return
    if (message.type === 'next') {
      if (!message.payload || Array.isArray(message.payload)) throw new Error('Invalid subscription result')
      if (message.payload.extensions?.subscriptionReady) this.settled(message.id)
      if (message.payload.errors?.length) console.error('GraphQL subscription errors', operation.field, message.payload.errors)
      if (message.payload.data && Object.hasOwn(message.payload.data, operation.field)) this.deliver(operation, message.payload.data[operation.field])
    } else if (message.type === 'error') {
      if (!Array.isArray(message.payload)) throw new Error('Invalid subscription error')
      console.error('GraphQL subscription failed', operation.field, message.payload)
      if (message.payload.some(error => error.message === 'resync_required')) {
        this.dropped(new Error('Subscription resynchronization required'), true)
        return
      }
      this.ids.delete(message.id)
      this.settled(message.id)
    } else if (message.type === 'complete') {
      this.ids.delete(message.id)
      this.settled(message.id)
    } else throw new Error('Unknown subscription message')
  }
  private settled(id: string) {
    const waiting = this.pending.delete(id)
    if (waiting && this.pending.size === 0) this.ready()
  }
  private dropped(error: unknown, resynchronize = false) {
    console.error(error)
    this.generation++
    this.connection?.close()
    this.acknowledged = false
    for (const listener of this.connectionListeners) listener(false)
    this.connection = undefined
    this.pending.clear()
    this.ids.clear()
    if (this.active && (resynchronize || !isNativeTarget())) this.retry = setTimeout(() => this.connect(), Math.min(5000, ++this.failures * 1000))
  }
  stop() {
    this.active = false
    for (const listener of this.connectionListeners) listener(null)
    this.generation++
    clearTimeout(this.retry)
    this.acknowledged = false
    this.connection?.close()
    this.connection = undefined
    this.pending.clear()
    this.ids.clear()
  }
}

export const graphqlSubscriptions = new SubscriptionClient()
