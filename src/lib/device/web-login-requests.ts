import { Channel, invoke } from '@tauri-apps/api/core'

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

type LoginSnapshot = {
  requests: Array<{
    requestId: string
    clientId: string
    clientIp: string
    browserName: string
    browserVersion: string
    osName: string
    osVersion: string
    verificationCode?: string | null
  }>
  nextAfter: string | null
}

export function subscribeWebLoginRequests(receive: (request: WebLoginRequest) => void, closed: () => void, dismiss: (requestId: string) => void) {
  if (!__IS_TAURI__) throw new Error('native_only')
  let disposed = false
  let ready = false
  let registration: number | undefined
  const buffered: Array<() => void> = []
  const deliver = (callback: () => void) => {
    if (disposed) return
    if (ready) callback()
    else buffered.push(callback)
  }
  void (async () => {
    const requested = new Channel<WebLoginRequest>()
    const closedChannel = new Channel<{ requestId: string }>()
    const snapshotChannel = new Channel<{ requests: WebLoginRequest[] }>()
    requested.onmessage = (payload) => deliver(() => receive(payload))
    closedChannel.onmessage = (payload) => deliver(() => dismiss(payload.requestId))
    snapshotChannel.onmessage = (payload) => deliver(() => {
      closed()
      payload.requests.forEach(receive)
    })
    const id = await invoke<number>('native_register_login_callbacks', { requested, closed: closedChannel, snapshot: snapshotChannel })
    if (disposed) {
      await invoke('native_unregister_login_callbacks', { id })
      return
    }
    registration = id
    let after: string | null = null
    do {
      const page: LoginSnapshot = await invoke('native_web_login_requests', { after })
      if (disposed) return
      for (const request of page.requests) {
        receive({
          requestId: request.requestId,
          clientId: request.clientId,
          clientIp: request.clientIp,
          request: {
            browserName: request.browserName,
            browserVersion: request.browserVersion,
            osName: request.osName,
            osVersion: request.osVersion,
            verificationCode: request.verificationCode ?? undefined,
          },
        })
      }
      after = page.nextAfter
    } while (after)
    ready = true
    buffered.forEach((callback) => callback())
  })().catch((error) => {
    if (!disposed) {
      if (registration !== undefined) void invoke('native_unregister_login_callbacks', { id: registration }).catch(console.error)
      console.error('Web login callbacks failed', error)
      closed()
    }
  })
  return { close() { disposed = true; if (registration !== undefined) void invoke('native_unregister_login_callbacks', { id: registration }).catch(console.error) } }
}

export function decideWebLogin(requestId: string, allowed: boolean) {
  return invoke<boolean>('native_confirm_web_login', { requestId, allowed })
}
