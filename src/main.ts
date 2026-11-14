import { bootstrapErrorDetail } from './bootstrap/bootstrap-error-detail'
import { runSelectedBootstrap } from './bootstrap/select-bootstrap'
import { installNativeReload } from './lib/api/native-invoke'
import { nativeViewScope } from './lib/api/native-client'

if (import.meta.env.DEV) window.__PLAIN_LOG__ = true

const pathname = window.location.pathname
void (async () => {
  if (__IS_TAURI__) {
    installNativeReload()
    await nativeViewScope()
  }
  await runSelectedBootstrap(pathname, __IS_TAURI__, {
    loadFullApp: async () => {
      const { bootstrapFullApp } = await import('./bootstrap/full-app')
      await bootstrapFullApp()
    },
    loadScreenCapture: async () => {
      const { bootstrapScreenCapture } = await import('./views/screen-capture/bootstrap')
      await bootstrapScreenCapture()
    },
  })
})().catch(async (error: unknown) => {
  console.error('application bootstrap failed', error)
  if (!__IS_TAURI__ || pathname !== '/screen-capture') return

  const overlayGeneration = Number(new URLSearchParams(window.location.search).get('overlayGeneration'))
  if (!Number.isSafeInteger(overlayGeneration) || overlayGeneration <= 0) return
  try {
    const { invoke } = await import('@/lib/api/native-invoke')
    await invoke('screen_capture_report_bootstrap_error', {
      overlayGeneration,
      detail: bootstrapErrorDetail(error),
    })
  } catch (reportError) {
    console.error('screen capture bootstrap failure could not be reported', reportError)
  }
})
