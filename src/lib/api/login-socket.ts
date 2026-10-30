import { openSocket } from './http'

type LoginFrame = string | ArrayBuffer | Blob

export async function openLoginSocket(url: string, signal?: AbortSignal) {
  const socket = openSocket(url)
  const frames: LoginFrame[] = []
  const pendingWork = new Set<(reason: string) => void>()
  let failure: string | undefined
  let waiting:
    | {
        resolve(frame: LoginFrame): void
        reject(reason: string): void
      }
    | undefined
  let resolveOpen: () => void
  let rejectOpen: (reason: string) => void
  const connected = new Promise<void>((resolve, reject) => {
    resolveOpen = resolve
    rejectOpen = reject
  })

  const cleanup = () => {
    signal?.removeEventListener('abort', abort)
    window.clearTimeout(connectionTimeout)
    window.clearTimeout(deadline)
  }
  const fail = (reason: string) => {
    if (failure) return
    failure = reason
    cleanup()
    rejectOpen(reason)
    waiting?.reject(reason)
    waiting = undefined
    for (const reject of pendingWork) reject(reason)
    pendingWork.clear()
    socket.close(3001, reason)
  }
  const abort = () => fail('cancelled')
  const connectionTimeout = window.setTimeout(() => fail('timeout'), 10000)
  const deadline = window.setTimeout(() => fail('login_expired'), 300000)

  socket.onopen = () => {
    window.clearTimeout(connectionTimeout)
    resolveOpen()
  }
  socket.onmessage = (event: MessageEvent<LoginFrame>) => {
    if (failure) return
    const size =
      typeof event.data === 'string'
        ? event.data.length
        : event.data instanceof ArrayBuffer
          ? event.data.byteLength
          : event.data.size
    if (size > 65536 || frames.length >= 16) {
      fail('invalid_auth_response')
      return
    }
    if (waiting) {
      waiting.resolve(event.data)
      waiting = undefined
    } else {
      frames.push(event.data)
    }
  }
  socket.onerror = () => fail('failed_connect_ws')
  socket.onclose = (event: CloseEvent) => fail(event.reason || 'failed')
  signal?.addEventListener('abort', abort, { once: true })
  if (signal?.aborted) abort()
  await connected

  const receive = async (): Promise<LoginFrame> => {
    if (failure) throw failure
    const frame = frames.shift()
    if (frame !== undefined) return frame
    return new Promise((resolve, reject) => {
      waiting = { resolve, reject }
    })
  }

  return {
    async waitFor<T>(work: Promise<T>): Promise<T> {
      return new Promise<T>((resolve, reject) => {
        if (failure) {
          reject(failure)
        } else {
          pendingWork.add(reject)
        }
        work.then(
          (value) => {
            pendingWork.delete(reject)
            resolve(value)
          },
          (error) => {
            pendingWork.delete(reject)
            reject(error)
          }
        )
      })
    },
    send(data: string | Uint8Array) {
      if (failure) throw failure
      socket.send(data as string | ArrayBufferView<ArrayBuffer>)
    },
    async receiveText() {
      const frame = await receive()
      if (typeof frame !== 'string' || frame.length > 4096) throw 'invalid_auth_response'
      return frame
    },
    async receiveBytes() {
      const frame = await receive()
      if (typeof frame === 'string') throw 'invalid_auth_response'
      const bytes = frame instanceof ArrayBuffer ? frame : await frame.arrayBuffer()
      if (failure) throw failure
      return new Uint8Array(bytes)
    },
    close(reason?: string) {
      if (reason) {
        fail(reason)
        return
      }
      if (failure) return
      failure = 'cancelled'
      cleanup()
      frames.length = 0
      socket.close()
    },
  }
}

export type LoginSocket = Awaited<ReturnType<typeof openLoginSocket>>
