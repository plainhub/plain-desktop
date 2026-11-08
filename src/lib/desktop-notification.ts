export type DesktopNotificationOptions = {
  title: string
  body?: string
  icon?: string
  silent?: boolean
}

export async function showDesktopNotification(options: DesktopNotificationOptions) {
  if (__IS_TAURI__) {
    const { isPermissionGranted, requestPermission } = await import('@/lib/tauri-plugins')
    let granted = await isPermissionGranted()
    if (!granted) {
      const permission = await requestPermission()
      granted = permission === 'granted'
    }
    if (!granted) return

    if (navigator.platform.toLowerCase().includes('mac')) {
      try {
        const { invoke } = await import('@tauri-apps/api/core')
        await invoke('send_macos_notification', { options })
        return
      } catch {
        // Fall through to the Tauri notification API if the macOS-specific
        // Tauri command isn't available in this build (e.g. dev mode
        // running the web bundle without the Tauri runtime wired up).
      }
    }

    new window.Notification(options.title, options)
    return
  }

  if ('Notification' in window && typeof Notification !== 'undefined' && Notification.permission === 'granted') {
    const notification = new Notification(options.title, {
      body: options.body,
      icon: options.icon,
      silent: options.silent,
    })
    notification.onclick = () => {
      window.focus()
      notification.close()
    }
  }
}