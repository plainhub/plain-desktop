import { invoke } from '@tauri-apps/api/core'
import { subscribeNativeEvents } from '@/lib/api/native-events'
import { parseWebSocketData } from '@/lib/api/sjcl-arraybuffer'
import { chachaDecrypt } from '@/lib/api/crypto'
import { tokenToKey } from '@/lib/api/file'
import { getLocalToken } from '@/lib/api/api'
import { eventAad } from '@/lib/api/websocket-events'

export interface WebLoginRequest {
  requestId: string
  clientId: string
  clientIp: string
  request: {
    browserName: string
    browserVersion: string
    osName: string
    osVersion: string
    verificationCode?: string
  }
}

export function subscribeWebLoginRequests(receive: (request: WebLoginRequest) => void, closed: () => void, dismiss: (requestId: string) => void) {
  if (!__IS_TAURI__) throw new Error('native_only')
  return subscribeNativeEvents((frame) => {
    const event = parseWebSocketData(frame)
    if (!['WEB_LOGIN_REQUESTED', 'WEB_LOGIN_CLOSED'].includes(event.type)) return
    const payload = JSON.parse(chachaDecrypt(tokenToKey(getLocalToken()), event.data, eventAad(event.type)))
    if (event.type === 'WEB_LOGIN_CLOSED') dismiss(payload.requestId)
    else receive(payload as WebLoginRequest)
  }, closed)
}

export function decideWebLogin(requestId: string, allowed: boolean) {
  return invoke<boolean>('native_confirm_web_login', { requestId, allowed })
}
