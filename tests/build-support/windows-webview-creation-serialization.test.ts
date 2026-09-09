import { describe, expect, it } from 'vitest'
import commandModules from '../../src-tauri/src/commands/mod.rs?raw'
import mediaPreviewSource from '../../src-tauri/src/commands/media_preview_pool.rs?raw'
import captureWindowSource from '../../src-tauri/src/commands/screen_capture/window.rs?raw'
import webviewCreationSource from '../../src-tauri/src/commands/webview_creation.rs?raw'
import windowSource from '../../src-tauri/src/commands/window.rs?raw'

describe('Windows dynamic WebView creation serialization', () => {
  it('keeps WebView2 construction off invoke handlers and behind one process-wide gate', () => {
    expect(commandModules).toContain('pub mod webview_creation;')
    expect(webviewCreationSource).toContain('static WEBVIEW_CREATION_LOCK: Mutex<()>')
    expect(webviewCreationSource).toContain('pub fn serialized<T>')

    expect(captureWindowSource).toContain('webview_creation::serialized')
    expect(mediaPreviewSource.match(/webview_creation::serialized/g)).toHaveLength(2)
    expect(windowSource).toContain('webview_creation::serialized')

    expect(mediaPreviewSource).toContain('pub async fn media_preview_init(')
    expect(mediaPreviewSource).toContain('pub async fn media_preview_activate(')
    expect(windowSource).toContain('pub async fn open_window(')
    const mediaPreviewCommands = mediaPreviewSource.slice(mediaPreviewSource.indexOf('#[tauri::command]'))
    expect(mediaPreviewCommands.match(/spawn_blocking/g)).toHaveLength(2)
    expect(windowSource).toContain('spawn_blocking')
  })
})
