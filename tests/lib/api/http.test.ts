import { describe, it, expect, vi, afterEach } from 'vitest'
import { httpRequest, openSocket } from '@/lib/api/http'

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('httpRequest (web transport)', () => {
  it('passes method, headers, body and signal to fetch', async () => {
    const fetchMock = vi.fn(async () => ({ status: 200, ok: true, arrayBuffer: async () => new ArrayBuffer(0) }))
    vi.stubGlobal('fetch', fetchMock)
    const controller = new AbortController()
    const body = new Uint8Array([1, 2, 3])

    const response = await httpRequest('http://localhost:3000/graphql', {
      method: 'POST',
      headers: { 'c-id': 'client-1' },
      body,
      signal: controller.signal,
    })

    expect(response.status).toBe(200)
    expect(fetchMock).toHaveBeenCalledOnce()
    const [url, init] = fetchMock.mock.calls[0]
    expect(url).toBe('http://localhost:3000/graphql')
    expect(init.method).toBe('POST')
    expect(init.headers).toEqual({ 'c-id': 'client-1' })
    expect(init.body).toBe(body)
    expect(init.signal).toBe(controller.signal)
  })

  it('exposes text() from the underlying response', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => ({ status: 200, ok: true, text: async () => 'hello', arrayBuffer: async () => new ArrayBuffer(0) })))
    const response = await httpRequest('http://localhost:3000/init', { method: 'POST' })
    await expect(response.text()).resolves.toBe('hello')
  })
})

describe('openSocket (web transport)', () => {
  it('returns a plain WebSocket in web mode', () => {
    const ws = openSocket('ws://localhost:1234')
    expect(ws).toBeInstanceOf(WebSocket)
    ws.close()
  })
})
