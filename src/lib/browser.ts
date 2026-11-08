export async function openUrl(url: string): Promise<void> {
  if (__IS_TAURI__) {
    if (url.startsWith('/') && !url.startsWith('//')) {
      const { openWindow } = await import('@/lib/api/tauri-window')
      return openWindow(url)
    }
    if (/^https?:\/\//i.test(url)) {
      const { openUrl: tauriOpenUrl } = await import('@/lib/tauri-plugins')
      return tauriOpenUrl(url)
    }
  }
  window.open(url, '_blank')
}
