import { getCurrentAuthToken } from '../device/current'
import { wrapWithReplayProtection } from './time-sync'
import { getApiBaseUrl, getApiHeaders } from './api'
import { chachaEncrypt, httpRequestAad, bitArrayToUint8Array } from './crypto'
import type { InitResponse } from './crypto'
import { tokenToKey } from './file'
import { httpRequest } from './http'

export interface InitResult {
  status: number
  data?: InitResponse
}

const pendingInit = new Map<string, Promise<InitResult>>()

export function requestInit(): Promise<InitResult> {
  const key = JSON.stringify([getApiBaseUrl(), getApiHeaders(), getCurrentAuthToken()])
  const pending = pendingInit.get(key)
  if (pending) return pending
  const request = fetchInit().finally(() => { pendingInit.delete(key) })
  pendingInit.set(key, request)
  return request
}

// Presents the stored token encrypted as an auto-login probe.
async function fetchInit(): Promise<InitResult> {
  const headers = getApiHeaders() as Record<string, string>
  let body: Uint8Array | undefined
  const token = getCurrentAuthToken()
  if (token) {
    const key = tokenToKey(token)
    body = bitArrayToUint8Array(chachaEncrypt(key, wrapWithReplayProtection('init'), httpRequestAad('/init')))
  }
  const initUrl = `${getApiBaseUrl()}/init`
  const r = await httpRequest(initUrl, { method: 'POST', headers, body })
  if (r.status === 403) return { status: r.status }
  const bodyText = await r.text()
  return { status: r.status, data: bodyText ? (JSON.parse(bodyText) as InitResponse) : undefined }
}
