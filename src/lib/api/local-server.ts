import { nativeGraphql } from '@/lib/api/native-client'
import { invoke } from '@tauri-apps/api/core'
import { httpRequest } from '@/lib/api/http'

export interface LocalServer {
  httpPort: number
  httpsPort: number
  urlToken: string
}

export interface HttpServerStarted {
  generation: number
  httpPort: number
  httpsPort: number
}

export interface HttpServerState {
  revision: number
  restarting: boolean
  httpPort: number
  httpsPort: number
  error: string
}

async function localData<T>(query: string, variables?: Record<string, unknown>, timeout = 30000): Promise<T> {
  const response = await nativeGraphql<{ data?: T; errors?: { message: string }[] }>({ query, variables }, timeout)
  if (response.errors?.length) throw new Error(response.errors[0].message)
  if (!response.data) throw new Error('Missing server response')
  return response.data
}

export async function readLocalServer(): Promise<LocalServer> {
  const data = await localData<{ app: LocalServer }>('{ app { httpPort httpsPort urlToken } }')
  if (!data.app || typeof data.app.urlToken !== 'string' || !validPort(data.app.httpPort) || !validPort(data.app.httpsPort)) throw new Error('Invalid server response')
  return data.app
}

export async function readLocalAddresses(): Promise<string[]> {
  const data = await localData<{ app: { network: { ipv4Addresses: string[] } } }>('{ app { network { ipv4Addresses } } }')
  if (!Array.isArray(data.app?.network?.ipv4Addresses)) throw new Error('Invalid network response')
  return data.app.network.ipv4Addresses
}

function validPort(value: number): boolean {
  return Number.isInteger(value) && value >= 0 && value <= 65535
}

function state(value: HttpServerState): HttpServerState {
  if (!value || !Number.isSafeInteger(value.revision) || typeof value.restarting !== 'boolean'
    || !validPort(value.httpPort) || !validPort(value.httpsPort) || typeof value.error !== 'string') {
    throw new Error('Invalid server state')
  }
  return value
}

export async function restartLocalServer(kind: 'http' | 'https', port: number): Promise<HttpServerState> {
  const deadline = Date.now() + 30000
  const receipt = state(await invoke<HttpServerState>('native_restart_http_server', { [`${kind}Port`]: port }))
  if (receipt.error) return receipt
  if (receipt.restarting || receipt.httpPort === 0) throw new Error('Invalid completed server restart')
  let lastFailure: unknown
  while (true) {
    const remaining = deadline - Date.now()
    if (remaining <= 0) throw Object.assign(new Error('connection_timeout'), { cause: lastFailure })
    try {
      const response = await httpRequest(`http://127.0.0.1:${receipt.httpPort}/health`, {
        signal: AbortSignal.timeout(Math.min(2000, remaining)),
      })
      const body = await response.text()
      if (response.ok && body === 'OK') return receipt
      lastFailure = new Error(response.ok ? 'Unexpected health response' : `Health check failed: HTTP ${response.status}`)
    } catch (error) { lastFailure = error }
    await new Promise((resolve) => setTimeout(resolve, Math.min(250, Math.max(0, deadline - Date.now()))))
  }
}
