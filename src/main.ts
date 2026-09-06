import { bootstrapErrorDetail } from './bootstrap/bootstrap-error-detail'
import { runSelectedBootstrap } from './bootstrap/select-bootstrap'

const pathname = window.location.pathname
void runSelectedBootstrap(pathname, __IS_TAURI__, {
  loadFullApp: async () => {
    const { bootstrapFullApp } = await import('./bootstrap/full-app')
    await bootstrapFullApp()
  },
  loadScreenCapture: async () => {
    const { bootstrapScreenCapture } = await import('./views/screen-capture/bootstrap')
    await bootstrapScreenCapture()
  },
}).catch(async (error: unknown) => {
  console.error('application bootstrap failed', error)
  if (!__IS_TAURI__ || pathname !== '/screen-capture') return

  const overlayGeneration = Number(new URLSearchParams(window.location.search).get('overlayGeneration'))
  if (!Number.isSafeInteger(overlayGeneration) || overlayGeneration <= 0) return
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    await invoke('screen_capture_report_bootstrap_error', {
      overlayGeneration,
      detail: bootstrapErrorDetail(error),
    })
  } catch (reportError) {
    console.error('screen capture bootstrap failure could not be reported', reportError)
  }
})
