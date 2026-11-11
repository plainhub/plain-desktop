import { proxyHttpUrl, proxyWsUrlFor } from './api'

export type HttpResponse = Response

export interface HttpRequestOptions {
  method?: string
  headers?: Record<string, string>
  body?: Uint8Array | null
  signal?: AbortSignal
}

/** Single transport choice for HTTP: the native fetch. In Tauri builds
 *  https device URLs are rewritten through the local reverse proxy because
 *  the webview rejects the devices' self-signed certificates; everything
 *  else is dialed directly (the desktop's own local server is plain HTTP
 *  on localhost). */
export async function httpRequest(url: string, options: HttpRequestOptions = {}): Promise<HttpResponse> {
  return fetch(proxyHttpUrl(url), {
    method: options.method,
    headers: options.headers,
    body: options.body as BodyInit | undefined,
    signal: options.signal,
  })
}

/** Single transport choice for sockets: the native WebSocket. In Tauri
 *  builds device sockets are rewritten to the local reverse proxy, which
 *  relays frames and re-resolves a `clientId` peer's current `ip:port`
 *  from the peers table right before dialing; loopback sockets (the
 *  desktop's own local server) connect directly. */
export function openSocket(url: string, clientId = ''): WebSocket {
  return new WebSocket(proxyWsUrlFor(url, clientId))
}
