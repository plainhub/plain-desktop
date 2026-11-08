/**
 * Opens a URL in the system's default browser.
 * In Tauri, window.open() spawns a new webview; this uses the opener plugin instead.
 * For internal app routes (starting with /), falls back to window.open.
 */
export async function openUrl(url: string): Promise<void> {
  if (__IS_TAURI__ && /^https?:\/\//i.test(url)) {
    const { openUrl: tauriOpenUrl } = await import('@/lib/tauri-plugins')
    await tauriOpenUrl(url)
  } else {
    window.open(url, '_blank')
  }
}
