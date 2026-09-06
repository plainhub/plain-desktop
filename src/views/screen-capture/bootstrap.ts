import type { UnlistenFn } from '@tauri-apps/api/event'
import { createApp, defineComponent, h, ref, shallowRef } from 'vue'
import ScreenCaptureOverlay, { type ScreenCaptureOverlayHandle } from './ScreenCaptureOverlay.vue'
import {
  createCaptureOverlaySession,
  type CaptureDeliveryFailed,
  type CaptureOverlayMount,
  type CaptureOverlayMountOptions,
  type CaptureOverlaySessionEnded,
  type CaptureTargetUnavailable,
} from './capture-overlay-session'
import { captureMessagesForLanguages, type CaptureMessages } from './capture-localization'
import type { CaptureFrameAvailable, CaptureInvoke, CaptureListen } from './capture-transport'
import { captureOverlayWindowLabel, createCaptureTransport, parseOverlayGeneration } from './capture-transport'
import { CAPTURE_OVERLAY_SESSION_ENDED_EVENT } from '@/lib/screen-capture/capture-events'
import { listenToCaptureWindow } from '@/lib/screen-capture/tauri-event-listener'
import './screen-capture.scss'

const TARGET_UNAVAILABLE_EVENT = 'screen-capture://target-unavailable'
const DELIVERY_FAILED_EVENT = 'screen-capture://delivery-failed'

function mountOverlay(root: HTMLElement, image: ImageData, options: CaptureOverlayMountOptions, messages: CaptureMessages): CaptureOverlayMount {
  const canConfirm = ref(options.canConfirm)
  const frame = shallowRef<ImageData | null>(image)
  const overlay = ref<ScreenCaptureOverlayHandle | null>(null)
  let disposed = false
  const app = createApp(
    defineComponent({
      name: 'ScreenCaptureBootstrapRoot',
      setup: () => () =>
        h(ScreenCaptureOverlay, {
          ref: overlay,
          frame: frame.value,
          canConfirm: canConfirm.value,
          messages,
          onExport: options.onExport,
          onCancel: options.onCancel,
          onFrameInstalled: () => {
            frame.value = null
          },
        }),
    })
  )

  root.replaceChildren()
  try {
    app.mount(root)
    const source = root.querySelector<HTMLCanvasElement>('.screen-capture-overlay__source')
    if (!overlay.value || source?.width !== image.width || source.height !== image.height) {
      throw new Error('screen capture frozen pixels were not installed')
    }
  } catch (error) {
    frame.value = null
    try {
      overlay.value?.dispose()
      app.unmount()
    } catch {
      // Preserve the mount/validation error that owns this cleanup path.
    }
    root.replaceChildren()
    throw error
  }

  return {
    setCanConfirm(value) {
      if (!disposed) canConfirm.value = value
    },
    dispose() {
      if (disposed) return
      disposed = true
      overlay.value?.dispose()
      app.unmount()
      root.replaceChildren()
    },
  }
}

export async function bootstrapScreenCapture(): Promise<void> {
  document.documentElement.classList.add('tauri', 'screen-capture')
  const root = document.querySelector<HTMLElement>('#app')
  if (!root) throw new Error('screen capture root is unavailable')

  root.dataset.bootstrap = 'screen-capture'
  const overlayGeneration = parseOverlayGeneration(window.location.search)
  const { getCurrentWebviewWindow } = await import('@tauri-apps/api/webviewWindow')
  const overlayWindowLabel = getCurrentWebviewWindow().label
  if (overlayWindowLabel !== captureOverlayWindowLabel(overlayGeneration)) {
    throw new Error('screen capture overlay window identity does not match its generation')
  }
  const messages = captureMessagesForLanguages(navigator.languages.length ? navigator.languages : [navigator.language])
  const [{ invoke }, { listen }] = await Promise.all([import('@tauri-apps/api/core'), import('@tauri-apps/api/event')])
  const tauriCaptureInvoke: CaptureInvoke = (command, args, options) => invoke(command, args, options)
  const tauriCaptureListen: CaptureListen = <T = CaptureFrameAvailable>(event: string, handler: (event: { payload: T }) => void | Promise<void>) =>
    listenToCaptureWindow<T>(listen, overlayWindowLabel, event, handler).then((unlisten: UnlistenFn) => unlisten)
  const captureInvoke = tauriCaptureInvoke
  const captureListen = tauriCaptureListen
  const overlaySession = createCaptureOverlaySession({
    overlayGeneration,
    invoke: captureInvoke,
    mount: (image, options) => mountOverlay(root, image, options, messages),
  })
  const unlisteners: UnlistenFn[] = []
  let transport: Awaited<ReturnType<typeof createCaptureTransport>> | null = null
  let disposed = false

  const dispose = () => {
    if (disposed) return
    disposed = true
    transport?.dispose()
    transport = null
    for (const unlisten of unlisteners.splice(0)) unlisten()
    overlaySession.dispose()
  }

  try {
    unlisteners.push(
      await captureListen<CaptureTargetUnavailable>(TARGET_UNAVAILABLE_EVENT, ({ payload }) => {
        overlaySession.targetUnavailable(payload)
      })
    )
    unlisteners.push(
      await captureListen<CaptureOverlaySessionEnded>(CAPTURE_OVERLAY_SESSION_ENDED_EVENT, ({ payload }) => {
        overlaySession.sessionEnded(payload)
      })
    )
    unlisteners.push(
      await captureListen<CaptureDeliveryFailed>(DELIVERY_FAILED_EVENT, ({ payload }) => {
        overlaySession.deliveryFailed(payload)
      })
    )
    transport = await createCaptureTransport({
      overlayGeneration,
      invoke: captureInvoke,
      listen: captureListen,
      present: (image, frame) => overlaySession.present(image, frame),
    })
  } catch (error) {
    dispose()
    throw error
  }

  window.addEventListener(
    'pagehide',
    () => {
      const unavailable = captureInvoke('screen_capture_unavailable', { overlayGeneration })
      dispose()
      void unavailable.catch(() => {
        // Native teardown or a newer overlay generation may already have won.
      })
    },
    { once: true }
  )
}
