import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { startOpaqueLogin, type OpaqueCommand, type OpaqueReply } from '../../src/lib/opaque/client'

class WorkerMock {
  static instances: WorkerMock[] = []
  onmessage?: (event: { data: OpaqueReply }) => void
  onerror?: () => void
  messages: OpaqueCommand[] = []
  postMessage = vi.fn((message: OpaqueCommand) => { this.messages.push(structuredClone(message)) })
  terminate = vi.fn()
  constructor() { WorkerMock.instances.push(this) }
  reply(data: OpaqueReply) { this.onmessage?.({ data }) }
}
beforeEach(() => {
  vi.useFakeTimers()
  WorkerMock.instances = []
  vi.stubGlobal('Worker', WorkerMock)
})
afterEach(() => { vi.unstubAllGlobals(); vi.useRealTimers() })

it.each(['crypto', 'Worker'])('fails closed without %s support, without a main-thread fallback', async missing => {
  vi.stubGlobal(missing, missing === 'crypto' ? {} : undefined)
  await expect(startOpaqueLogin('synthetic password')).rejects.toBe('secure_login_unavailable')
  expect(WorkerMock.instances).toHaveLength(0)
})

it('uses a dedicated lazy worker and clears the sender password bytes', async () => {
  expect(WorkerMock.instances).toHaveLength(0)
  const started = startOpaqueLogin('synthetic password')
  const worker = WorkerMock.instances[0]
  expect(worker.messages[0].type).toBe('start')
  const password = (worker.messages[0] as Extract<OpaqueCommand, { type: 'start' }>).password
  expect(new TextDecoder().decode(password)).toBe('synthetic password')
  expect((worker.postMessage.mock.calls[0][0] as Extract<OpaqueCommand, { type: 'start' }>).password.every(byte => byte === 0)).toBe(true)
  worker.reply({ type: 'request', request: new Uint8Array([1]) })
  const login = await started
  expect(login.request()).toEqual(new Uint8Array([1]))
  const finished = login.finish(new Uint8Array([2]), 'browser', new Uint8Array(32))
  const result = { ke3: new Uint8Array([3]), apiKey: new Uint8Array(32), requestAad: new Uint8Array(), responseAad: new Uint8Array(), serverPublicKey: new Uint8Array(32) }
  worker.reply({ type: 'result', result })
  await expect(finished).resolves.toEqual(result)
  expect(worker.terminate).toHaveBeenCalledOnce()
  await expect(login.finish(new Uint8Array(), 'browser', new Uint8Array())).rejects.toBe('invalid_auth_response')
  expect(vi.getTimerCount()).toBe(0)
})

it.each(['start', 'finish'])('terminates the worker immediately when canceled during %s', async stage => {
  const controller = new AbortController()
  const started = startOpaqueLogin('synthetic password', controller.signal)
  const worker = WorkerMock.instances[0]
  let operation: Promise<unknown> = started
  if (stage === 'finish') {
    worker.reply({ type: 'request', request: new Uint8Array([1]) })
    const login = await started
    operation = login.finish(new Uint8Array([2]), 'browser', new Uint8Array(32))
  }
  const rejected = expect(operation).rejects.toBe('canceled')
  controller.abort()
  await rejected
  expect(worker.terminate).toHaveBeenCalledOnce()
  expect(vi.getTimerCount()).toBe(0)
})

it('terminates state after a worker error and on login expiry', async () => {
  const first = startOpaqueLogin('synthetic password')
  const error = expect(first).rejects.toBe('invalid_auth_response')
  WorkerMock.instances[0].onerror?.()
  await error
  expect(WorkerMock.instances[0].terminate).toHaveBeenCalledOnce()
  const second = startOpaqueLogin('synthetic password')
  const expired = expect(second).rejects.toBe('login_expired')
  await vi.advanceTimersByTimeAsync(300000)
  await expired
  expect(WorkerMock.instances[1].terminate).toHaveBeenCalledOnce()
  expect(vi.getTimerCount()).toBe(0)
})
