import { describe, expect, it } from 'vitest'
import applicationSource from '../../src-tauri/src/lib.rs?raw'
import cargoManifest from '../../src-tauri/Cargo.toml?raw'
import captureCommandsSource from '../../src-tauri/src/commands/screen_capture/commands.rs?raw'
import captureModuleSource from '../../src-tauri/src/commands/screen_capture/mod.rs?raw'
import windowSource from '../../src-tauri/src/commands/screen_capture/window.rs?raw'
import captureBootstrapSource from '../../src/views/screen-capture/bootstrap.ts?raw'
import captureTransportSource from '../../src/views/screen-capture/capture-transport.ts?raw'

describe('Windows capture overlay session residency', () => {
  it('creates a fresh authenticated overlay for each capture instead of reusing a cold WebView2 document', () => {
    const overlayBuilder = windowSource.slice(
      windowSource.indexOf("fn overlay_builder<'a>"),
      windowSource.indexOf('fn show_overlay_for_bootstrap')
    )
    const windowsCreation = windowSource.slice(
      windowSource.indexOf('fn create_windows_overlay'),
      windowSource.indexOf('impl<R: Runtime> CaptureWindowPort')
    )
    const residencyPolicy = windowSource.slice(
      windowSource.indexOf('fn uses_ephemeral_overlay'),
      windowSource.indexOf('fn conceal_overlay_for_capture')
    )
    const terminalConcealment = windowSource.slice(
      windowSource.indexOf('fn conceal_overlay('),
      windowSource.indexOf('fn restore_window(')
    )

    expect(applicationSource).toMatch(
      /#\[cfg\(not\(target_os = "windows"\)\)\][\s\S]{0,600}screen capture overlay prewarm started/
    )
    expect(residencyPolicy).toContain('cfg!(target_os = "windows")')
    expect(cargoManifest).not.toContain('"unstable"')
    expect(cargoManifest).not.toContain('webview2-com')
    expect(cargoManifest).not.toContain('windows-core')
    expect(windowsCreation).toContain('Self::overlay_builder(&self.app, spec)')
    expect(windowsCreation).toContain('.build()')
    expect(windowsCreation).toContain('.inner_size(')
    expect(windowsCreation).toContain('.transparent(false)')
    expect(windowsCreation).toContain('.visible(true)')
    expect(windowsCreation).not.toContain('tauri::WebviewBuilder::new(')
    expect(windowsCreation).not.toContain('.with_environment(')
    expect(windowsCreation).not.toContain('tauri::WindowBuilder::new(')
    expect(windowsCreation).not.toContain('.add_child(')
    expect(windowSource).not.toContain('windows_bridge')
    expect(windowsCreation).not.toContain('run_on_main_thread(move ||')
    expect(windowSource).toContain('creating serialized Windows capture overlay')
    expect(overlayBuilder).not.toContain('.data_directory(')
    expect(overlayBuilder).not.toContain('.additional_browser_args(')
    expect(terminalConcealment).toContain('OverlayConcealment::RetirementScheduled')
    expect(terminalConcealment).toContain('destroy retired capture overlay')
    expect(windowSource).not.toContain('fn refreshes_overlay_after_capture(&self) -> bool')
    expect(applicationSource).not.toContain('WINDOWS_CAPTURE_PROTOCOL_SCHEME')
    expect(captureModuleSource).not.toContain('windows_bridge')
    expect(captureCommandsSource).not.toContain('screen_capture_wait_for_frame')
    const bootstrapFailureHandler = captureCommandsSource.slice(
      captureCommandsSource.indexOf('pub fn screen_capture_report_bootstrap_error'),
      captureCommandsSource.indexOf('pub fn screen_capture_register_target')
    )
    expect(bootstrapFailureHandler).toContain('overlay_bootstrap_failed')
    expect(bootstrapFailureHandler).toContain('window.destroy()')
    expect(captureTransportSource).not.toContain('CaptureDeliveryMode')
    expect(captureTransportSource).not.toContain('windows-bridge')
    expect(captureTransportSource).not.toContain('native-wait')
    expect(captureTransportSource).not.toContain("deliveryMode === 'poll'")
    expect(captureBootstrapSource).not.toContain('deliveryMode:')
  })
})
