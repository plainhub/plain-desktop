export function desktopReleaseConfig(target) {
  const source = `../target/desktop-release-runtime/${target}/`
  const mac = target.endsWith('-apple-darwin')
  const runtime = mac ? 'libonnxruntime.dylib'
    : target.endsWith('-windows-msvc') ? 'onnxruntime.dll' : 'libonnxruntime.so'
  const bundle = { resources: {
    [source + 'LICENSE']: 'onnxruntime/LICENSE',
    [source + 'ThirdPartyNotices.txt']: 'onnxruntime/ThirdPartyNotices.txt',
  } }
  if (mac) {
    bundle.macOS = { entitlements: './entitlements.plist.identity', frameworks: [source + runtime] }
  } else {
    bundle.resources[source + runtime] = runtime
  }
  return { bundle }
}
