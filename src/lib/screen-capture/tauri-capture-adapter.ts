import { CaptureClientError, getCaptureClient, type CaptureClient, type CaptureConsumerRegistration, type CaptureStartResponse } from './capture-client'
import { listenToCaptureWindow } from './tauri-event-listener'

export interface ChatCaptureDestination {
  readonly chatId: string
  readonly channelId: string
  readonly appDir: string
}

export type ChatCaptureConsumer = (file: File, destination: ChatCaptureDestination) => Promise<void>

export interface ChatCaptureTarget {
  activate(destination: ChatCaptureDestination): string
  deactivate(): void
  start(destination: ChatCaptureDestination): Promise<CaptureStartResponse>
  dispose(): void
}

function errorDetail(error: unknown, depth = 0, seen = new Set<object>()): string {
  if (depth >= 4) return '[cause depth exceeded]'
  if (error instanceof Error) {
    if (seen.has(error)) return '[circular error cause]'
    seen.add(error)
    const own = `${error.name}: ${error.message}`
    return error.cause === undefined ? own : `${own}; cause: ${errorDetail(error.cause, depth + 1, seen)}`
  }
  if (typeof error === 'string') return error
  try {
    return JSON.stringify(error)
  } catch {
    return String(error)
  }
}

export function formatCaptureClientError(error: CaptureClientError): string {
  const cause = error.cause === undefined ? '' : `; cause: ${errorDetail(error.cause)}`
  return `${error.code}: ${error.message}${cause}`.slice(0, 1024)
}

export function isRegularCaptureWindowLabel(label: string): boolean {
  return label === 'main' || (label.startsWith('window-') && label.length > 'window-'.length)
}

async function invokeCaptureErrorReport(detail: string): Promise<void> {
  const [{ invoke }, { getCurrentWindow }] = await Promise.all([import('@tauri-apps/api/core'), import('@tauri-apps/api/window')])
  if (!isRegularCaptureWindowLabel(getCurrentWindow().label)) return
  await invoke('screen_capture_report_client_error', { detail: detail.slice(0, 1024) })
}

export async function reportTauriCaptureError(context: string, error: unknown): Promise<void> {
  const detail = `${context}: ${errorDetail(error)}`.slice(0, 1024)
  try {
    await invokeCaptureErrorReport(detail)
  } catch {
    console.error(detail, error)
  }
}

function requireNonBlank(value: string, field: string): void {
  if (!value.trim()) throw new Error(`capture ${field} is required`)
}

export function snapshotChatCaptureDestination(destination: ChatCaptureDestination): ChatCaptureDestination {
  requireNonBlank(destination.chatId, 'chat id')
  requireNonBlank(destination.appDir, 'app directory')
  return Object.freeze({
    chatId: destination.chatId,
    channelId: destination.channelId,
    appDir: destination.appDir,
  })
}

function sameDestination(left: ChatCaptureDestination | null, right: ChatCaptureDestination): boolean {
  return left?.chatId === right.chatId && left.channelId === right.channelId && left.appDir === right.appDir
}

/**
 * Owns the chat consumer registration separately from CaptureClient's native
 * session. Replacing an activation creates both a new immutable destination
 * closure and a new opaque target token.
 */
export function createChatCaptureTarget(client: CaptureClient, consume: ChatCaptureConsumer): ChatCaptureTarget {
  let registration: CaptureConsumerRegistration | null = null
  let destination: ChatCaptureDestination | null = null
  let active = false
  let disposed = false

  function replace(next: ChatCaptureDestination): string {
    const frozen = snapshotChatCaptureDestination(next)
    const previous = registration
    registration = null
    destination = null
    active = false
    previous?.deactivate()
    previous?.dispose()
    const nextRegistration = client.registerConsumer((file) => consume(file, frozen))
    let token: string
    try {
      token = nextRegistration.activate()
    } catch (error) {
      nextRegistration.dispose()
      throw error
    }
    registration = nextRegistration
    destination = frozen
    active = true
    return token
  }

  return {
    activate(next) {
      if (disposed) throw new Error('capture target is disposed')
      return replace(next)
    },
    deactivate() {
      if (!active) return
      active = false
      registration?.deactivate()
    },
    async start(next) {
      if (disposed) throw new Error('capture target is disposed')
      if (!active) throw new Error('capture target is not active')
      const frozen = snapshotChatCaptureDestination(next)
      if (!sameDestination(destination, frozen)) replace(frozen)
      return client.startComposerCapture()
    },
    dispose() {
      if (disposed) return
      disposed = true
      active = false
      registration?.dispose()
      registration = null
      destination = null
    },
  }
}

/** Load and install the per-webview client without any eager Tauri imports. */
export async function getTauriCaptureClient(onError: (error: CaptureClientError) => void): Promise<CaptureClient> {
  const [{ invoke }, { listen }, { getCurrentWindow }] = await Promise.all([import('@tauri-apps/api/core'), import('@tauri-apps/api/event'), import('@tauri-apps/api/window')])
  const windowLabel = getCurrentWindow().label
  if (!isRegularCaptureWindowLabel(windowLabel)) {
    throw new CaptureClientError('target_unavailable', 'screen capture is unavailable in utility windows')
  }
  return getCaptureClient({
    windowLabel,
    invoke,
    listen: async (event, handler) =>
      listenToCaptureWindow(listen, windowLabel, event, (incoming) => {
        void handler({ payload: incoming.payload })
      }),
    onError: (error) => {
      const message = `screen capture client failure: ${formatCaptureClientError(error)}`
      // Route diagnostics through an application command. Calling the log
      // plugin directly can be rejected by a production capability policy,
      // which previously reduced every failure to an unactionable toast.
      void invokeCaptureErrorReport(message).catch(() => {
        console.error(message, error)
      })
      onError(error)
    },
  })
}
