import { expect, it } from 'vitest'
import { desktopReleaseConfig } from '../../scripts/desktop-release-config.mjs'

it.each([
  ['aarch64-apple-darwin', 'libonnxruntime.dylib'],
  ['x86_64-apple-darwin', 'libonnxruntime.dylib'],
  ['x86_64-pc-windows-msvc', 'onnxruntime.dll'],
  ['x86_64-unknown-linux-gnu', 'libonnxruntime.so'],
])('includes the compiled ONNX Runtime and license for %s', (target, library) => {
  const { bundle } = desktopReleaseConfig(target)
  const source = `../target/desktop-release-runtime/${target}/`
  expect(bundle.resources[source + 'LICENSE']).toBe('onnxruntime/LICENSE')
  expect(bundle.resources[source + 'ThirdPartyNotices.txt']).toBe('onnxruntime/ThirdPartyNotices.txt')
  if (target.endsWith('-apple-darwin')) {
    expect(bundle.macOS?.frameworks).toEqual([source + library])
    expect(bundle.macOS?.entitlements).toBe('./entitlements.plist.identity')
  } else {
    expect(bundle.resources[source + library]).toBe(library)
  }
})
