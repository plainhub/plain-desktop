import { tauriFetch, type TauriFetchResponse } from './tauri-fetch'
import { TauriWebSocket } from './tauri-ws'

export type HttpResponse = TauriFetchResponse

export interface HttpRequestOptions {
  method?: string
  headers?: Record<string, string>
  body?: Uint8Array | null
  signal?: AbortSignal
}

/** Single transport choice for HTTP: on Tauri, https URLs go through the
 *  Rust reqwest client because the webview rejects the devices' self-signed
 *  certificates; everything else uses the native fetch. */
export async function httpRequest(url: string, options: HttpRequestOptions = {}): Promise<HttpResponse> {
  if (__IS_TAURI__ && url.startsWith('https://')) {
    const request = tauriFetch(url, options)
    return options.signal ? raceAbort(request, options.signal) : request
  }
  return fetch(url, {
    method: options.method,
    headers: options.headers,
    body: options.body as BodyInit | undefined,
    signal: options.signal,
  })
}

/** Single transport choice for sockets: on Tauri every socket is relayed by
 *  Rust (self-signed certificates accepted; a non-empty `clientId` re-resolves
 *  the peer's current ip:port from the peers table right before dialing). */
export function openSocket(url: string, clientId = ''): WebSocket {
  if (__IS_TAURI__) return new TauriWebSocket(url, clientId) as unknown as WebSocket
  return new WebSocket(url)
}

/** tauriFetch's invoke() cannot be aborted — race the signal so caller
 *  timeouts still fire; the abandoned request simply completes unseen. */
function raceAbort<T>(request: Promise<T>, signal: AbortSignal): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const onAbort = () => reject(new DOMException('Aborted', 'AbortError'))
    if (signal.aborted) return onAbort()
    signal.addEventListener('abort', onAbort, { once: true })
    request.then(
      (value) => {
        signal.removeEventListener('abort', onAbort)
        resolve(value)
      },
      (error) => {
        signal.removeEventListener('abort', onAbort)
        reject(error)
      },
    )
  })
}
