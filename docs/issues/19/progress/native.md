# P0 native capture progress

Status: the native coordinator, async acquisition path, Wayland portal adapter, and global-target handshake are implemented and compile/unit-tested. End-to-end KDE/X11 capture, Windows shortcut plus Chat-composer capture, and macOS Apple-Silicon/Retina capture now pass. This is not feature-completion evidence: pure-Wayland, mixed-DPI hardware, macOS Intel/non-Retina, and release-signed package gates remain open.

## Baseline and drift

- Feature base: upstream `main` at `52531249b6c31983fb6c5c89a001eabfaf5bbf8b`.
- Plan commit: `109b1de9390333a2cdae87ad027f5f05f7105dc9`.
- `git fetch upstream main` on 2026-09-02 left `upstream/main` at the feature base; there is no upstream drift to integrate.
- The pre-existing `plain-rs` Git dependency moved after the baseline lock was written. P0 records and locks commit `77e7e4c8c60d418d8557efa1403c90c262e8c9e0`.

## Contract corrections found before freeze

Adversarial reviews found four issues in the first plan draft. The plan and P0 contracts now require:

1. `/screen-capture` dispatch before any full-app imports rather than a shared-router route.
2. Separate `CaptureOrigin`, `CaptureTarget`, `NativeCapturePhase`, and frontend UI phase ownership.
3. One-shot raw-frame reads, but retryable authenticated PNG results guarded by one delivery-attempt lease at a time; failure releases the lease and success acknowledges/removes the result.
4. An async `ChatView` result consumer with destination IDs snapshotted before the first await; Vue emits cannot acknowledge an async upload.

Tauri capability files govern plugin/core APIs in this repository, but its custom commands do not yet have a generated `AppManifest` ACL. The overlay will therefore receive minimal plugin/core permissions and every capture custom command will validate caller label plus session ownership. Migrating all existing custom commands to generated ACL permissions is recorded as separate security debt.

## Pinned dependency decision

| Dependency | Version | License | Decision |
|---|---:|---|---|
| `xcap` | 0.9.8 | Apache-2.0 | No default features; leave `image` and `wgc` off pending platform-specific proof |
| `tauri-plugin-global-shortcut` | 2.3.2 | MIT OR Apache-2.0 | Rust-owned Windows/macOS/X11 registration; no guest package or capability |
| `tauri-plugin-clipboard-manager` | 2.3.3 | MIT OR Apache-2.0 | Rust-owned, process-long clipboard adapter; no guest package or capability |
| `ashpd` (Linux only) | 0.13.13 | MIT | `screencast` + `global_shortcuts` + `tokio`, for the owned Wayland capture and shortcut portal adapters |
| `pipewire` (Linux only) | 0.10.1 | MIT | Direct dependency used to acquire and validate one bounded ScreenCast frame |
| `libwayshot-xcap` (transitive) | 0.3.3 | BSD-2-Clause | Accepted as xcap's Linux capture backend |

`cargo metadata --locked --format-version 1` found no new Git-sourced package. The only Git source remains the repository's pre-existing `plain-rs` dependency. The full graph also contains existing MPL-2.0 web-stack crates; `cargo-deny`/`cargo-about` remains a release gate.

## Linux build prerequisites

The check and release workflows install:

```text
pkg-config
libclang-dev
libxcb1-dev
libxrandr-dev
libdbus-1-dev
libpipewire-0.3-dev
libwayland-dev
libegl-dev
libgbm-dev
```

All were present on the validation host. Exact installed versions were:

```text
pkg-config              1.8.1-2build1
libclang-dev            1:18.0-59~exp2
libxcb1-dev             1.15-1ubuntu2
libxrandr-dev           2:1.5.2-2build1
libdbus-1-dev           1.14.10-4ubuntu4.1
libpipewire-0.3-dev      1.0.5-1ubuntu3.3
libwayland-dev          1.22.0-2.1build1
libegl-dev              1.7.0-1build1
libgbm-dev              25.2.8-0ubuntu0.24.04.2
```

## Real X11 capture evidence

Disposable probe: `/tmp/codex/plain-issue-19-native-probe` (not product code).

Commands:

```bash
timeout 120s cargo check --locked
timeout 180s cargo run --locked
```

Host/session: KDE Plasma 5.27.12, X11, `DISPLAY=:0`, two 2560×1440 monitors.

Results:

- Both monitors enumerated.
- Both full captures returned exact 14,745,600-byte RGBA buffers.
- Full-monitor capture latency was 228 ms and 229 ms; complete two-monitor probe was 480 ms.
- A 64×64 region succeeded on each monitor.
- Out-of-bounds regions were rejected.
- No pixel data was logged or persisted.

## Binary IPC, async acquisition, and contract evidence

Failing-first tests were introduced before each implementation slice. The initial failures were missing contract, backend, buffer, binary IPC, session-guard, and platform modules. After implementation:

- 10 contract tests cover coordinate spaces, negative origins, half-open monitor selection, origin/target independence, native phase separation, stride/length checks, integer overflow, and the 256 MiB cap.
- 5 backend tests cover re-enumeration/hotplug, empty/outside monitor selection, pre-allocation size rejection, invalid native dimensions, permission denial, and capture failure propagation.
- 5 buffer tests cover one-shot frames, stale sessions, single-reader retryable results, explicit release/acknowledgment, sensitive-buffer cleanup, PNG metadata, and 4K checksum/length.
- 2 raw IPC tests use Tauri's real `InvokeBody`/`Response` types and prove deterministic 1080p and 4K byte length/checksum without JSON serialization.
- 3 session-guard tests reject empty, repeated, concurrent, and stale session operations.
- 7 platform tests cover Windows physical geometry, macOS/X11 logical-to-physical scaling, mixed-DPI macOS selection, Wayland fail-closed selection, stale-output tolerance, typed permission denial, negative origins, invalid scale, and overflow.
- 7 coordinator tests cover caller roles, exact overlay generations, ready-before-start and start-before-ready ordering, phase guards, one-shot frames, retryable single-reader result leases, and terminal sensitive-buffer cleanup.
- Async runtime tests prove that cancellation and the watchdog can acquire the coordinator while native acquisition is pending, and that a frame completing after either terminal edge is rejected as stale.
- The xcap acquisition-gate test proves that an uncancellable timed-out `spawn_blocking` call retains the process-wide lease until the backend actually exits; a later xcap session fails closed instead of entering the display backend concurrently.
- Wayland decoder and monitor tests cover Tauri-derived physical/logical/scale metadata, exact and fail-closed stream matching, RGBA/BGRA/X formats, row padding, negative stride, non-zero chunk offsets, malformed chunks, and pre-copy size limits.
- Native-ID selection tests cover stale/duplicate xcap candidates and mixed-DPI monitor matching without rescaling global coordinates into the wrong display space.
- Global-target tests cover regular-window-only registration, destroyed-window pruning, focused-window preference with latest-eligible fallback, listener-ready ordering, authenticated session metadata, exact-token invalidation, and target loss during delivery.

Focused command examples:

```bash
cargo +1.96.0 test --locked --manifest-path src-tauri/Cargo.toml --lib commands::screen_capture::contract_tests
cargo +1.96.0 test --locked --manifest-path src-tauri/Cargo.toml --lib commands::screen_capture::ipc_tests
cargo +1.96.0 test --locked --manifest-path src-tauri/Cargo.toml --lib commands::screen_capture::platform_tests
```

The raw IPC fixture test takes approximately one second locally, including construction/checksum of a 33,177,600-byte 4K RGBA frame.

Broader local gates on 2026-09-02 after the async/Wayland/mixed-DPI/global-target integration:

```text
cargo +1.96.0 test --locked --manifest-path src-tauri/Cargo.toml --lib
  250 passed; 0 failed; 0 ignored; 1.02 s test execution

cargo +1.96.0 check --locked --manifest-path src-tauri/Cargo.toml
  passed; warnings are pre-existing plus capture contract helpers used only on other targets or in tests

corepack yarn typecheck
  passed

corepack yarn build
  passed

env VITE_APP_MODE=tauri corepack yarn build
  passed

focused screen-capture browser tests
  165 passed

Tauri-mode ChatInput capture component tests
  3 passed

corepack yarn test
  682 passed; 52 skipped; only the same 3 allowlisted baseline failures
```

The locked Linux release package gate also passed with the repository's pinned
Rust 1.96 toolchain:

```text
RUSTUP_TOOLCHAIN=1.96.0 VITE_APP_MODE=tauri corepack yarn tauri build -- --locked
  passed; produced .deb, .rpm, and AppImage bundles
```

The host's default Rust 1.94 compiler correctly rejected the package before
compilation because `rust-version = "1.96"`; explicitly selecting the same
toolchain pinned in CI completed the build. These are local compile,
unit/component-test, and packaging results. They do not exercise a real
Wayland portal/PipeWire session, signed macOS/Windows artifacts, or remote
GitHub runners.

The repository-wide `cargo fmt --check` is not currently a usable gate: upstream Rust is broadly formatted differently from Rust 1.96's edition-2024 output. Every new `screen_capture` Rust file passes a direct Rust 1.96 `rustfmt --edition 2024 --check`; no unrelated source was reformatted.

## Assembled application integration

The production Tauri builder now owns one `ScreenCaptureRuntime`, installs the native clipboard adapter, conditionally installs the ordinary shortcut plugin only outside pure Wayland, retains the Wayland portal shortcut guard, prewarms a fixed hidden overlay on Linux/macOS, creates ephemeral overlays on demand on Windows, registers the capture commands, and routes close/destroy events through synchronous capture cleanup. The macOS dock menu excludes the capture utility window.

Capture reservation is now separated from acquisition. Native code freezes the session/generation first, schedules readiness and lifetime watchdogs, performs the short compositor-unmap settle asynchronously, and runs the slow backend without holding the runtime mutex. Cancellation, timeout, and window lifecycle hooks therefore remain able to clear the session while capture is pending; a late frame is discarded against its exact ticket.

The global shortcut has an explicit target handshake rather than guessing a chat after capture:

- An active `ChatView` installs its listeners, registers an opaque rotating target token with native code, and unregisters/invalidate the exact token when that target deactivates or is disposed.
- Native code accepts target registration only from regular application webviews. A global trigger prunes destroyed registrations, prefers an eligible target in the focused Plain window, falls back to the latest eligible target, freezes that `{ window label, target token }`, and emits authenticated session metadata before acquisition begins.
- Result reads, releases, acknowledgments, and target invalidation require the frozen caller window, session, result, and token. The overlay receives only session/generation availability metadata; it never receives the target token.
- The chat consumer freezes its immutable upload destination at activation, so later route/thread changes cannot redirect the captured `File`.

An adversarial integration pass found and corrected two launch/lifecycle issues before landing:

- An ordinary global-shortcut collision originally propagated out of Tauri setup and could prevent Plain Desktop from launching. Registration failure now leaves the composer button available and emits a warning without claiming success.
- A delayed `screen_capture_init` call ran from every full-app utility window, including the media-preview webview, and produced an unauthorized-caller warning. Native startup now performs the single Linux/macOS prewarm; Windows creates an ephemeral overlay on demand, and the redundant browser command was removed.
- Exact session/generation watchdogs now bound overlay readiness to 10 seconds and the complete ephemeral capture lifetime to 15 minutes. Stale timers and readiness timers that race a healthy active overlay are no-ops.
- The runtime snapshots each origin window's visibility, minimized, and focus state before hiding it, then restores that exact state on every terminal edge. A transient restoration failure keeps only bounded metadata, retries after 50/200/500 ms, and never retains screenshot pixels or wedges the process-wide coordinator.
- Native page-load hooks distinguish the overlay's first load from a reload/navigation. A later load start restores the origin and clears sensitive buffers before the old JavaScript heap can strand a session; retained Linux/macOS overlays can rearm after navigation, while Windows retires its ephemeral generation.

A complete Tauri dev application compiled and launched on the KDE/X11 host with its fixed hidden overlay and no capture initialization warning. The desktop was locked when interactive overlay testing began, so the session was stopped without bypassing the lock; this is build/startup evidence, not a claim of completed end-to-end UI proof.

An `x86_64-apple-darwin` Rust standard library was installed and a locked cross-target check was attempted. The Linux host cannot complete that check because it has no Apple SDK/cross-linker: the transitive Objective-C helper reaches the host `cc`, which rejects Apple's `-arch` and macOS flags before Plain's code is compiled. Disabling `cc` produces the same dependency build-script boundary. This is recorded as a host-toolchain limitation, not as macOS proof; the macOS CI runner remains required.

The Mac App Store entitlement now grants `com.apple.security.files.user-selected.read-write`, matching capture export through the user-selected native save destination. This is a configuration correction only; it is not proof that a signed/sandboxed App Store build can save successfully.

## Wayland implementation and evidence boundary

This host session remains KDE/X11, so no pure-Wayland runtime claim is made. The KDE portal advertises GlobalShortcuts v1, Screenshot v2, and ScreenCast v5. The implemented Wayland path deliberately does not reuse xcap's Linux enumeration/cropping behavior:

- Wayland is detected before `XcapBackend` is constructed. Monitor candidates come only from Tauri/winit's `available_monitors`, so a genuine Wayland session does not enter xcap/XCB enumeration.
- `ashpd` opens an explicit XDG ScreenCast portal chooser. The selected stream is acquired directly through PipeWire, bounded by separate portal-interaction and first-frame timeouts, normalized into tightly packed RGBA, and validated for format, dimensions, stride, chunk offset/size, and the process memory cap.
- Portal stream metadata must map unambiguously to one Tauri monitor; ambiguous or malformed metadata fails closed rather than placing the overlay on a guessed display.
- The non-Wayland xcap acquisition remains behind the process-wide acquisition gate because its blocking call cannot be cancelled after timeout.
- Wayland global shortcuts use the XDG GlobalShortcuts portal; the ordinary Tauri shortcut plugin is not initialized when either Wayland session signal is present.

This path compiles on Linux and its conversion/matching contracts are unit-tested. Actual chooser behavior, PipeWire negotiation, focus/unmap timing, multi-monitor selection, denial/cancel handling, and global-shortcut activation still require an installed build in KDE Wayland; GNOME Wayland remains a separate runtime gate.

## CI and packaging gates

The check workflow is configured to run frontend typecheck, both web and Tauri frontend builds, focused capture tests in Chromium, and the Tauri-only composer test. Its Rust matrix uses Rust 1.96 and `--locked` for macOS, Windows, and Linux checks and focused capture tests; Linux additionally runs the complete Rust library suite. Chromium and its system packages are installed only after the cheaper static/build gates pass.

Both check and release workflows install the Linux xcap/PipeWire development stack listed above. Desktop and App Store build scripts pass Cargo `--locked`, and release packaging remains sequential within each matrix job. These workflow/configuration changes and the local commands above have been inspected; successful remote CI and produced artifacts remain open evidence, not assumed results.

## Known open lifecycle and platform work

The implementation is intentionally not marked complete. The compositor settle delay, initial overlay focus/first pointer-down behavior, close/exit ordering, overlay destruction/rebuild, rapid retrigger, target-close, monitor-hotplug, and exact window-state restoration paths have unit coverage but still need interactive packaged stress testing.

The following P0 gates also remain open until appropriate hosts/CI or packaged builds are available:

- KDE and GNOME Wayland portal/shortcut/runtime proof.
- Windows Messages-composer/send/copy/save and mixed-DPI proof; macOS Intel,
  non-Retina, global-shortcut, copy/save, and release-signed-package proof.
- Real mixed-DPI monitor matching on Linux, Windows, and macOS hardware; current native-ID and coordinate tests are synthetic.
- Packaged native clipboard ownership/persistence and save-dialog behavior.
- Successful cross-platform GitHub check/release runs and smoke-tested produced packages.

## KDE/X11 assembled-runtime evidence (2026-09-03)

A production-mode binary built with the pinned Rust 1.96 toolchain was launched
on KDE Plasma/X11 and exercised against the local chat service:

- The composer scissors action and `Alt+A` both opened the annotation overlay.
- Selection, annotation, PNG rendering, native result handoff, upload, and chat
  insertion completed end to end.
- A 100 ms origin-window delay captured a partially faded Plain window under
  KWin. Raising only the Linux compositor-unmap settle to 250 ms removed the
  origin window from the captured frame; Windows and macOS retain 100 ms.
- The first successful upload still displayed a false `Failed` toast. Native
  diagnostics identified an `invalid_result` in the main webview after the
  upload had completed. The target lifecycle and overlay lifecycle were both
  using `screen-capture://session-ended` with incompatible payloads; Tauri's
  application-level event delivery allowed the target client to consume the
  overlay payload. The two lifecycles now use distinct event names, with a
  regression test asserting the target client never subscribes to the overlay
  terminal channel.
- The hidden `media-preview-warm` utility webview also attempted to register
  as a capture target. Capture initialization now rejects every label except
  `main` and non-empty `window-*` labels before installing listeners or
  touching the native registry.
- After those corrections, an end-to-end capture uploaded and appeared in
  local chat with no error toast and no capture diagnostic in the application
  log. The two diagnostic captures created by automated verification were
  removed afterward.

The same assembled build exposes capture in the SMS/Messages composer and
routes the frozen PNG through the existing MMS send path while preserving the
user's text and attachment draft. That integration has component/unit proof;
an actual carrier MMS send remains a device/runtime gate.

Current focused evidence after the corrections:

```text
VITE_APP_MODE=tauri corepack yarn test <capture/chat/message test set>
  166 passed

RUSTUP_TOOLCHAIN=1.96.0 cargo test --manifest-path src-tauri/Cargo.toml screen_capture --locked
  108 passed

VITE_APP_MODE=tauri corepack yarn typecheck
  passed

corepack yarn eslint <changed frontend files>
  passed

corepack yarn build
VITE_APP_MODE=tauri corepack yarn build
  passed

corepack yarn test
  692 passed; 52 skipped; only the same 3 allowlisted baseline failures
```

## Windows debugger-confirmed correction (2026-09-05)

Early Windows runs exposed real packaging, renderer-lifecycle, and command-order
hazards. Those experiments were useful for narrowing the failure, but their
parked-renderer polling, native WebView2 messaging/shared buffers, forced
navigation, separate data directories, split window/child-WebView construction,
and fixed destruction delays were tested and rejected. They are not part of the
final implementation.

CDB captured the decisive failure: a synchronous Tauri command was constructing
a media-preview WebView while WRY held its process-wide `WebContext` store
mutex and pumped Windows messages. The capture callback then tried to construct
another WebView and re-entered that non-reentrant mutex. This explains the
apparently frozen or failed capture launch. Windows capture permission was not
the cause.

The final correction is deliberately small and uses normal Tauri primitives:

- Every dynamic WebView creation path shares one Windows serialization gate.
- Media-preview and ordinary-window invoke commands are asynchronous and move
  their blocking window construction into `spawn_blocking`.
- Windows capture overlays are ephemeral, generation-bound ordinary
  `WebviewWindow` instances. Each starts as a mapped opaque, click-through
  one-pixel bootstrap anchor and expands only after its frozen frame is decoded.
  Windows no longer prewarms an indefinitely parked capture renderer.
- Frame delivery uses targeted Tauri events on every platform. The callback
  queues work with `setTimeout(0)`, performs one pending-frame recovery after
  readiness, and deduplicates a bounded set of session identifiers.
- A frontend bootstrap failure retires that generation, defers its WebView
  destruction until after the reporting invoke returns, and releases the
  retirement gate even if the platform refuses destruction. Generation-bound
  labels let the next explicit capture recover without reusing the failed
  document or remaining permanently `Busy`.
- The final tree contains no custom capture URI, shared-buffer bridge, native
  WebView polling, renderer-residency hack, or direct WebView2 dependencies.

Final automated validation after the Windows correction:

```text
cargo +1.96.0 test --locked --manifest-path src-tauri/Cargo.toml --lib -j 2
  268 passed

corepack yarn vitest run \
  tests/lib/screen-capture tests/views/screen-capture \
  tests/views/chat/chat-input-capture.test.ts \
  tests/views/messages/message-chat-capture.test.ts \
  tests/build-support/app-mode.test.ts \
  tests/build-support/windows-capture-ephemeral-overlay.test.ts \
  tests/build-support/windows-webview-creation-serialization.test.ts
  172 passed

corepack yarn typecheck
cargo +1.96.0 check --locked --manifest-path src-tauri/Cargo.toml -j 2
corepack yarn build:tauri:frontend
Windows cargo build --locked --features tauri/custom-protocol -j 2
  passed

corepack yarn test
  708 passed; 52 skipped; the same 3 allowlisted baseline failures
```

On the Windows VM, the final `Alt+A` path created a fresh serialized overlay,
completed native acquisition and raw-frame reading, positioned the overlay at
1600x1200, and received a successful presentation acknowledgment. A QEMU
framebuffer capture independently confirmed that the WebView painted the frozen
desktop. A subsequent manual Chat-composer run created a new generation, hid
the origin, accepted a region selection and confirmation, published the chat
event, and acknowledged destruction of the ephemeral overlay without a failure
toast. A separate
startup-race run deliberately overlapped media-preview and capture WebView
creation and also completed, directly exercising the corrected serialization
boundary. Messages-composer delivery, mixed-DPI/negative-origin hardware,
packaged clipboard/save behavior, and a long idle soak remain explicit
follow-up runtime gates.

## macOS Apple-Silicon/Retina runtime evidence (2026-09-07)

The fork commit `b59d6ec1f385831c1fac946d07989ba786c62ad6` and the
subsequent toolbar work-area correction were checked, tested, packaged,
ad-hoc signed, and exercised on `wyatt-mac`: macOS 26.5.2, Apple M5/arm64,
Xcode 26.6, and Rust 1.96.0.

```text
cargo +1.96.0 check --locked --manifest-path src-tauri/Cargo.toml -j 2
  passed

cargo +1.96.0 test --locked --manifest-path src-tauri/Cargo.toml --lib -j 2
  256 passed; 0 failed

cargo +1.96.0 build --locked --features tauri/custom-protocol -j 2
  passed

DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer \
  RUSTUP_TOOLCHAIN=1.96.0 cargo tauri build --debug --bundles app \
  --config '{"build":{"beforeBuildCommand":""}}'
  passed; produced PlainApp.app

codesign --force --deep --sign - \
  --identifier com.ismartcoding.plain.desktop PlainApp.app
codesign --verify --deep --strict PlainApp.app
  passed

corepack yarn vitest run \
  tests/lib/screen-capture tests/views/screen-capture \
  tests/views/chat/chat-input-capture.test.ts \
  tests/views/messages/message-chat-capture.test.ts \
  tests/build-support/app-mode.test.ts \
  tests/build-support/windows-capture-ephemeral-overlay.test.ts \
  tests/build-support/windows-webview-creation-serialization.test.ts
  179 passed; 0 failed

corepack yarn test
  719 passed; 52 skipped; the same 3 allowlisted baseline failures

Linux cargo +1.96.0 test --locked --manifest-path \
  src-tauri/Cargo.toml --lib -j 2
  271 passed; 0 failed
```

The first test incorrectly started the executable inside the bundle directly.
CoreGraphics denied capture, but that launch did not register the bundle with
LaunchServices, so PlainApp was absent from Privacy & Security. Relaunching the
same signed bundle through `open` registered its path and bundle identifier.
The next capture produced a `kTCCServiceScreenCapture` record; macOS 26 logged
that the service does not allow an in-app prompt and exposed PlainApp for
manual enablement instead. The separate “find nearby devices” dialog was the
local-network permission used by Plain device discovery, not screen recording.

After the user enabled Screen Recording and macOS terminated the application,
the bundle was relaunched through LaunchServices. The fixed hidden overlay
prewarmed, the 3420x2214 Retina display was captured and mapped to a
1710x1107 logical overlay at scale 2, the raw frame read succeeded, and overlay
presentation was acknowledged. A subsequent user-initiated composer capture
repeated acquisition and presentation; the user selected and confirmed a
region and reported that the workflow worked. The user also proved that the
documented Option+Command+A global shortcut dispatches correctly.

The first interactive build waited 100 ms after hiding Plain before acquiring
pixels. That was long enough for AppKit to hide the window logically, but a
faint outline from its close animation remained in the captured frame. Matching
the Linux compositor allowance at 250 ms removed the outline; the user reported
that the final timing works correctly.

The initial toolbar correction used WebKit's `screen.avail*` geometry. It kept
the toolbar away from physical display edges, but this macOS WebKit build
reported the Dock-covered area as available. An AppKit probe measured the
1710x1107 logical display as a bottom-left `NSScreen.visibleFrame` of
`x=0, y=76, width=1710, height=997`, which converts to overlay-local top-left
`x=0, y=34, width=1710, height=997`: 34 points reserved for the menu bar and 76
for the Dock. The final implementation retrieves that authoritative visible
frame from the positioned overlay's `NSWindow.screen` on AppKit's main thread
through a generation-authenticated Tauri command. It converts the coordinates,
tries toolbar placement below, above, right, and left of the selection, and
finally clamps inside the visible work area. Other platforms, a missing screen,
or any rejected/malformed query safely fall back to browser geometry.

The final ad-hoc-signed bundle was launched through LaunchServices. The user
manually verified that the toolbar avoids both screen edges and the macOS Dock
and that all controls remain clickable, reporting that the result works great.
The disposable automatic-trigger harness was removed and generated schema files
were restored after the runtime checks.

This proves Apple-Silicon/Retina permission denial, manual grant, required
restart, native acquisition, frame delivery, overlay presentation, selection,
confirmation, global-shortcut dispatch, compositor-hide timing, and Dock-aware
toolbar placement in an ad-hoc-signed debug bundle. Re-signing an ad-hoc test
build changes its code-directory hash, so macOS treats each rebuild as a new TCC
subject and requires a reset/regrant; a stable release-signed application does
not change signing identity on every local rebuild. This evidence does not claim
Intel, non-Retina, mixed-display, clipboard/save persistence, App Store sandbox,
Developer-ID signing/notarization, or release artifact proof.

## Xenocept provenance

Xenocept source was audited at private commit `35efe0e` with the repository
owner's explicit permission. This implementation adapts its backend/coordinator
separation and evidence about platform failure modes. No Xenocept source file
was copied. Its older dependency pins, canvas editor, AeorDB integration, plugin
system, HTTP/eval transport, radial UI, and persisted screenshot history remain
intentionally excluded.
