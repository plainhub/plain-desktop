import { createServer as createHttpServer, request } from 'node:http'
import type { AddressInfo } from 'node:net'
import { createServer } from 'vite'
import { expect, it, vi } from 'vitest'

it('forwards same-origin development requests for a managed device to the configured backend', async () => {
  const received = vi.fn()
  const backend = createHttpServer(async (req, res) => {
    const body: Buffer[] = []
    for await (const chunk of req) body.push(chunk)
    received(req.url, req.headers.origin, req.headers.host, req.headers['c-id'], Buffer.concat(body).toString())
    res.writeHead(req.headers.origin === backendOrigin ? 200 : 403).end()
  })
  backend.on('upgrade', (req, socket) => {
    received(req.url, req.headers.origin, req.headers.host, req.headers['sec-websocket-protocol'])
    socket.end('HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n')
  })
  const closeBackend = () => new Promise<void>((resolve, reject) => backend.close(error => error ? reject(error) : resolve()))
  let port: number
  do {
    await new Promise<void>(resolve => backend.listen(0, '127.0.0.1', resolve))
    port = (backend.address() as AddressInfo).port
    if (port % 100 === 43) await closeBackend()
  } while (port % 100 === 43)
  const host = '127.0.0.1:' + port
  const backendOrigin = 'http://' + host
  vi.stubEnv('VITE_APP_API_HOST', host)
  vi.stubEnv('VITE_APP_MODE', 'web')
  const vite = await createServer({
    mode: 'development',
    optimizeDeps: { noDiscovery: true },
    server: { host: '127.0.0.1', port: 0, hmr: false, watch: null },
  })
  try {
    await vite.listen(0)
    const origin = 'http://127.0.0.1:' + (vite.httpServer!.address() as AddressInfo).port
    const path = '/proxy/fixture/graphql'
    const post = (source: string) => fetch(origin + path, {
      method: 'POST', headers: { Origin: source, 'c-id': 'fixture-browser' }, body: 'fixture-envelope',
    })
    expect((await post(origin)).status).toBe(200)
    expect(received).toHaveBeenCalledExactlyOnceWith(path, backendOrigin, host, 'fixture-browser', 'fixture-envelope')
    const websocketStatus = (source: string) => new Promise<number | undefined>((resolve, reject) => {
      const upgrade = request(origin + path + '?cid=fixture-browser', {
        headers: {
          Origin: source, Connection: 'Upgrade', Upgrade: 'websocket',
          'Sec-WebSocket-Protocol': 'fixture', 'Sec-WebSocket-Version': '13',
          'Sec-WebSocket-Key': Buffer.alloc(16).toString('base64'),
        },
      }, response => { response.resume(); resolve(response.statusCode) })
      upgrade.on('error', reject)
      upgrade.setTimeout(2000, () => upgrade.destroy(new Error('Upgrade timed out')))
      upgrade.end()
    })
    expect(await websocketStatus(origin)).toBe(401)
    expect(received).toHaveBeenLastCalledWith(path + '?cid=fixture-browser', backendOrigin, host, 'fixture')
    expect((await post('http://foreign.test')).status).toBe(404)
    expect(await websocketStatus('http://foreign.test')).toBe(404)
    expect(received).toHaveBeenCalledTimes(2)
    expect((await fetch(origin + '/fs?id=fixture', {
      headers: { 'Sec-Fetch-Site': 'cross-site' },
    })).status).toBe(404)
    expect(received).toHaveBeenCalledTimes(2)
    expect((await fetch(origin + '/fs?id=fixture')).status).toBe(200)
    expect(received).toHaveBeenLastCalledWith('/fs?id=fixture', backendOrigin, host, undefined, '')
  } finally {
    vi.unstubAllEnvs()
    await vite.close()
    await closeBackend()
  }
})
