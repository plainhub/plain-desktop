# WeChat-like screen capture and annotation

## Tracking

- Issue: <https://github.com/plainhub/plain-desktop/issues/19>
- Repository: <https://github.com/plainhub/plain-desktop>
- Feature baseline: `52531249b6c31983fb6c5c89a001eabfaf5bbf8b`
- Current upstream integrated: `d0f1574d015300c1359b233055753a145e52ed85` (`v0.1.11`)
- Original feature branch: `th317erd:feat/issue-19-screen-capture`
- Merged feature pull request: <https://github.com/plainhub/plain-desktop/pull/20>
- Permission follow-up branch: `th317erd:fix/issue-19-macos-permission-guide`
- Permission follow-up pull request: <https://github.com/plainhub/plain-desktop/pull/21>

## Priority and scope

Issue #19 requests a WeChat-like desktop capture and annotation workflow using
visual mockups. The implementation is confined to `plain-desktop`: no Android
application, server/GraphQL contract, PlainApp web bundle, or `plain-rs` API was
changed for the feature.

The included scope is transient monitor capture, rectangular selection,
annotation, native save/copy, chat and SMS/Messages composer delivery, global
shortcuts, error recovery, localization, and desktop work-area avoidance on
Windows, macOS, and Linux.

Explicitly excluded are screenshot history, AeorDB, Xenocept's plugin system,
its HTTP/eval transport, multi-monitor stitching, cursor capture, a second
annotation engine, and changes to the note/Markdown editor.

## Requested behavior and baseline

The issue mockups show a scissors action that opens a full-screen capture
overlay, allows a region to be selected and annotated, and provides undo,
redo, save, copy, cancel, and confirm actions. The requested global shortcut is
`Alt+A` on Windows/Linux and `Option+Command+A` on macOS.

At the baseline commit, Plain Desktop had no capture action, global capture
shortcut, native capture coordinator, capture overlay, or transient annotation
session. Plain's image editor already contained the useful annotation and
rendering primitives, but they were coupled to persistent project state and
could not safely be used for an ephemeral screenshot.

## Investigation findings

### Existing Plain seams

- `ChatInput.vue` already emitted image files through the chat upload path.
- The SMS/Messages composer already accepted attachments through its existing
  MMS path.
- Plain's image editor already defined rectangle, ellipse, arrow, freehand,
  text, mosaic, rendering, and undo semantics.
- Ordinary utility webviews booted application services that a capture overlay
  must not start.
- Cached chat routes meant that a global event broadcast to every composer
  could deliver a capture to the wrong conversation.
- Tauri JSON events were unsuitable for raw 4K monitor frames.
- Platform work areas, coordinate origins, and scale factors differ across
  Win32, AppKit, X11/GDK, and Wayland portal capture.

### Xenocept provenance

The contributor-owned Xenocept repository was audited at
`35efe0eab84ba8bf26e1fc575b2db40302685bb7` with the owner's explicit
permission. Its backend/coordinator separation and documented platform failure
modes informed this design. No Xenocept source file was copied. Xenocept's
AeorDB storage, plugin host, capture HTTP API, screenshot history, radial UI,
comments, notes, and persisted canvas model were deliberately omitted.

### Failures found during implementation

Interactive testing exposed and corrected several failures before submission:

- Linux initially captured Plain's partially hidden window because compositor
  unmapping had not completed.
- Chat delivery succeeded but then showed a false failure toast because two
  lifecycle channels reused one event name with incompatible payloads.
- A hidden media-preview webview attempted to register as a capture target.
- Windows could deadlock while media preview and capture created WebViews
  concurrently through WRY's non-reentrant WebContext storage.
- Reusing a failed Windows overlay could leave later captures permanently busy
  or blank.
- macOS permission discovery failed when the executable was launched outside
  LaunchServices, and the hide delay left a faint animated window outline.
- Maintainer testing with `yarn dev:tauri` on macOS 26 correctly reached the
  native `PermissionDenied` path, but Plain discarded that typed result and
  reduced it to an unactionable `Failed` toast. The same error was absent from
  the target-window terminal event used by global-shortcut captures.
- Toolbar placement based only on browser geometry could put controls behind
  the macOS Dock, Windows taskbar, or Linux panel.
- Upstream's new i18n compiler rejected the capture locale glob's alias form.
- Vite 8.0.16's Rolldown 1.0.3 dependency optimizer generated an unbound Vue
  initialization call on a clean browser-test cache.

## Implemented correction

### Native capture and lifecycle

- Added a Rust-owned, process-wide capture runtime with explicit session,
  generation, origin-window, delivery-target, result-lease, restoration, and
  sensitive-buffer state.
- Added bounded, validated raw-frame capture for Win32/macOS/X11 and an owned
  XDG ScreenCast/PipeWire path for pure Wayland.
- Added platform shortcuts while keeping the ordinary X11 shortcut plugin out
  of pure Wayland sessions.
- Added one-shot binary frame reads and authenticated, retryable PNG result
  delivery. Pixels and target tokens are never placed in URLs or logs.
- Serialized all dynamic WebView creation. Windows uses disposable,
  generation-bound overlays; Linux and macOS prewarm a minimal hidden overlay.
- Added exact window-state restoration, bounded retry, readiness/lifetime
  watchdogs, stale-result rejection, and cleanup on navigation, close,
  destruction, cancellation, or backend failure.
- Added native clipboard/save support and the Mac App Store entitlement for a
  user-selected save destination.

### Overlay and annotation

- Added a minimal `/screen-capture` bootstrap that does not import the full
  application, router, Pinia, sockets, discovery, GraphQL, or media-preview
  initialization.
- Extracted Plain's existing rendering and annotation behavior behind an
  in-memory `useAnnotationSession()` boundary. The full image editor retains
  its persistence adapters; capture installs none.
- Added selection normalization, resizing, dimensions, exterior dimming,
  rectangle/ellipse/arrow/pen/text/mosaic tools, five colors, three stroke
  sizes, undo/redo, keyboard cancellation, clipped PNG rendering, and explicit
  resource disposal.
- Added localized capture-only messages for all 17 existing locales.
- Positions the toolbar below, above, right, or left of the selection and then
  clamps it to the native monitor work area, avoiding display edges, menu bars,
  docks, taskbars, and panels.

### Composer delivery

- Added Tauri-only scissors actions to both Chat and SMS/Messages composers.
- Active chat views register opaque, rotating delivery tokens. Native code
  freezes the eligible target at capture start; the frontend freezes the
  immutable conversation/upload destination before awaiting any work.
- Confirm routes exactly one `image/png` file through the existing chat upload
  or MMS attachment path. Save and copy remain available without a chat target;
  confirm explains why it is unavailable.
- Failed upload/export attempts preserve the capture and draft for retry.

### macOS permission recovery

- Preserves the native `permission_denied` machine code through both rejected
  composer starts and failed global-shortcut terminal events.
- Replaces the generic toast for that macOS failure with an animated Plain
  modal that explains the exact Privacy & Security pane, the PlainApp toggle,
  and the required application restart.
- Adds an **Open System Settings** action for the Screen & System Audio
  Recording pane. The native command accepts no URL from the webview, uses one
  hard-coded macOS settings URL, and rejects capture-overlay or other utility
  window callers.
- Keeps every unrelated failure and every non-macOS permission failure on the
  existing generic error path.

### Upstream integration and CI reproducibility

Upstream `main` through `a26dd7e3` was merged without rewriting branch history.
Conflict resolution preserved upstream's chat windowing, async window commands,
i18n compilation, and `v0.1.10` changes together with capture isolation and
WebView serialization.

The capture locale glob now uses a compiler-safe relative path. Vitest projects
receive independent compile-time define objects. Vite is pinned to 8.1.0,
whose Rolldown line contains the upstream fix for the clean-cache Vue optimizer
failure, and dynamically loaded Tauri test dependencies are pre-optimized.

## Regression coverage

The added frontend suites prove:

- bootstrap isolation and Tauri/web mode selection;
- capture target registration, invalidation, authentication, and retry;
- immutable chat/MMS destinations and upload error recovery;
- selection geometry in every drag direction;
- all requested tools, gesture rollback, text drafts, undo/redo, and disposal;
- clipped PNG output, localization, native work-area placement, and toolbar
  fallbacks;
- Windows ephemeral overlay and global WebView-creation serialization rules.
- typed permission propagation for direct and shortcut starts, single-instance
  permission-guide presentation, non-macOS fallback behavior, and the exact
  native settings command invoked by the guide.

The Rust suites prove:

- coordinate, monitor, scale, stride, overflow, memory-cap, and frame contracts;
- one-shot frame ownership, retryable result leases, and sensitive cleanup;
- concurrent/repeated/stale session rejection;
- backend re-enumeration and typed permission/capture failures;
- Wayland stream matching and bounded RGBA/BGRA/X frame conversion;
- lifecycle races, late results, target loss, timeouts, restoration, overlay
  retirement/rebuild, and shortcut platform selection;
- native work-area conversion for Windows, Linux, and macOS.
- failed terminal metadata carries only the bounded native error code, while
  the hard-coded settings command rejects non-application webviews.

## Validation

### Current merged Linux tree

```text
corepack yarn install --immutable
corepack yarn typecheck
corepack yarn build
corepack yarn build:tauri:frontend
  passed

CI-equivalent focused browser gate from a clean optimizer cache
  201 passed; 0 failed

VITE_APP_MODE=tauri composer gate
  4 passed; 0 failed

cargo +1.96.0 check --locked --manifest-path src-tauri/Cargo.toml -j 2
  passed

cargo +1.96.0 test --locked --manifest-path src-tauri/Cargo.toml --lib -j 2
  267 passed; 0 failed

corepack yarn test
  813 passed; 52 skipped; 3 known baseline failures

isolated cross-window project
  5 passed; 0 failed
```

The full frontend failure count did not grow. The three stable failures are the
pre-existing web/local-mode expectations recorded in the implementation plan.
Mechanical architecture searches and `git diff --check` pass.

### Post-merge macOS permission follow-up

Maintainer testing after pull request #20 was merged reproduced a denied capture
under the exact `yarn dev:tauri` workflow on macOS 26. The native log retained
the useful `PermissionDenied` code and `permission_prompt_returned=false`, but
the frontend erased that code before presenting the generic `Failed` toast.
Pull request #21 preserves the code through direct and global-shortcut capture
paths and presents the recovery guide described above.

The follow-up was replayed onto upstream `main` at `d0f1574d` rather than merged
back from the squash-merged feature branch. Validation from that tree produced:

```text
focused permission/capture/composer frontend tests
  70 passed; 0 failed

corepack yarn typecheck
corepack yarn build:tauri:frontend
  passed

cargo +1.96.0 test --locked --manifest-path src-tauri/Cargo.toml --lib -j 2
  271 passed; 0 failed
```

The upstream Windows-firewall commit had accidentally removed the source line
for the Git-based `plain-rs` package from `Cargo.lock`, causing every existing
`--locked` CI command to fail before compilation. The follow-up regenerates only
that one source line and proves the repaired lockfile with the command above.

A clean Apple Silicon/macOS checkout passed the 70 focused frontend tests, all
256 platform-eligible Rust library tests, typecheck, Tauri frontend build, and
macOS-only Rust compilation. The exact `yarn dev:tauri` command then compiled
and launched `target/debug/PlainApp`. Inspection confirmed that Tauri embedded
the configured Info.plist and that the process used the expected ad-hoc
development identity. Xcode 26.6's SDK does not define an
`NSScreenCaptureUsageDescription` property-list key, so an invented privacy key
was not added; denied-state recovery remains an application guide to Apple's
Screen & System Audio Recording pane followed by a PlainApp restart.

### Interactive platform proof

- KDE Plasma/X11, two 2560x1440 monitors: scissors and `Alt+A`, selection,
  annotation, upload into local chat, cancellation, compositor timing, and
  panel-aware toolbar placement passed. Linux release `.deb`, `.rpm`, and
  AppImage packaging passed before the final upstream merge.
- Windows 11/QEMU, 1600x1200: scissors and `Alt+A`, capture presentation,
  annotation, Chat confirmation, startup overlap with media preview, taskbar
  avoidance, and restoration passed. The complete pre-merge Windows Rust suite
  passed with 259 tests.
- macOS 26.5.2 on Apple M5/arm64 Retina: ad-hoc-signed app packaging and
  verification, permission denial/grant/restart, scissors and
  `Option+Command+A`, capture/selection/confirmation, hide timing, and
  Dock/menu-bar avoidance passed. The complete pre-merge macOS Rust suite passed
  with 259 tests.

## Remaining limitations

- The post-merge permission follow-up must pass the Windows/macOS/Linux GitHub
  Actions matrix and be retested by the maintainer in the original denied TCC
  state.
- KDE/GNOME pure-Wayland chooser, PipeWire negotiation, and portal shortcut
  behavior have contract tests but still need interactive packaged proof.
- Real mixed-DPI/negative-origin multi-monitor hardware remains covered by
  synthetic tests rather than every physical platform combination.
- Windows SMS/MMS carrier delivery, packaged clipboard/save persistence,
  macOS Intel/non-Retina, App Store sandboxing, Developer-ID signing/notarizing,
  release-signed artifacts, and a long idle/retrigger soak remain unclaimed.
- The full frontend suite retains three unrelated baseline failures documented
  above; the PR's focused CI gate is green locally.
