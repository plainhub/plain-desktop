export class RequestQueue {
  private active = 0
  private waiting: Array<() => void> = []

  constructor(private readonly capacity: number) {}

  run<T>(execute: () => Promise<T>, signal?: AbortSignal): Promise<T> {
    return new Promise((resolve, reject) => {
      const cancel = () => {
        this.waiting = this.waiting.filter(request => request !== start)
        reject(new DOMException('Request cancelled', 'AbortError'))
      }
      const start = () => {
        signal?.removeEventListener('abort', cancel)
        this.active++
        void (async () => {
          try { resolve(await execute()) }
          catch (error) { reject(error) }
          finally { this.active--; this.next() }
        })()
      }
      if (signal?.aborted) { cancel(); return }
      signal?.addEventListener('abort', cancel, { once: true })
      this.waiting.push(start)
      this.next()
    })
  }

  private next() {
    if (this.active < this.capacity) this.waiting.shift()?.()
  }
}
