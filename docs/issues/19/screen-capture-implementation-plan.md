# Issue #19: WeChat-like screen capture and annotation

- Status: implementation complete; pull request preparation in progress
- Target repository: `plainhub/plain-desktop` only
- Feature branch: `feat/issue-19-screen-capture`
- Upstream baseline: `52531249b6c31983fb6c5c89a001eabfaf5bbf8b` (2026-09-02)
Issue: <https://github.com/plainhub/plain-desktop/issues/19>

## Target

Add a Tauri-only, WeChat-style desktop screen-capture flow that captures the monitor under the pointer, lets the user select and annotate a region, and can save, copy, cancel, or send the resulting PNG through the existing chat image-upload pipeline.

The work is judged by the behavior shown in issue #19, reliable packaged behavior on Plain Desktop's Windows/macOS/Linux targets, and mechanical proof that capture remains transient and does not introduce a second annotation engine or a new persistence system.

## Owner constraints and rulings

The following user rulings were recorded verbatim on 2026-09-02:

> “Since Xenocept is owned by me, I want to borrow as much code as possible to implement this feature he is requesting.”

> “Obviously we won't be pulling in the whole plugin system Xenocept has, or some of its other features... and we WILL NOT be relying on AeorDB for storage...”

> “This is for the `plain-desktop` app only.”

Consequences:

- Xenocept code owned by the contributor may be adapted and contributed to Plain Desktop under Plain Desktop's MIT license.
- Every adapted block must be recorded in the final investigation report with its Xenocept source file and audited revision. This is a provenance ledger, not a runtime dependency.
- No PlainApp Android change, server/GraphQL contract, AeorDB storage, Xenocept plugin host, local capture HTTP API, screenshot history, or delivery plugin is allowed.
- Capture pixels and annotation state are ephemeral. They must never enter the PlainApp image-project store, Yjs network transport, Pinia persistence, URLs, logs, or GraphQL.

## Issue-derived acceptance contract

Issue #19 contains mockups rather than written acceptance criteria. The following is the literal behavior inferred from those mockups; additions beyond it are marked separately.

1. A `content-cut` scissors `v-icon-button` appears after the image and folder actions in `ChatInput.vue` only in Tauri builds. Web builds do not render it.
2. The button and the global shortcut use one capture coordinator. The requested shortcuts are `Alt+A` on Windows/Linux and `Option+Command+A` on macOS.
3. Activation freezes one complete monitor and shows it in a borderless, monitor-sized overlay. The monitor under the pointer at activation is selected.
4. The user drag-selects one region. The region is normalized in every drag direction, clamped to that monitor, surrounded by a visible border/handles, and accompanied by live pixel dimensions. The exterior is dimmed. A selection cannot cross a display boundary by construction.
5. Releasing a valid selection reveals the compact toolbar. It contains rectangle, ellipse, arrow, pen/freehand, text, mosaic, undo, redo, five color presets, three stroke sizes, save, copy, cancel, and confirm.
6. Save writes a PNG chosen through a native save dialog. Copy writes PNG pixels to the native clipboard. Cancel creates no artifact. Confirm produces exactly one `File` with MIME `image/png` and sends it through the invoking chat input's existing `send-images` path.
7. Save and copy close only after success. A native/export error keeps the capture and annotations intact so the user can retry.
8. Escape cancels the current text/gesture first, then exits capture. `Ctrl/Cmd+Z` and `Ctrl/Cmd+Shift+Z` undo and redo. Escape never sends, copies, saves, or persists.
9. Only one process-wide capture session may be active. A second trigger focuses the existing overlay rather than racing a second capture.
10. Completion, cancellation, every error path, and an origin-window close restore any window hidden by the capture coordinator and release sensitive image buffers.

Self-decided product details:

- The captured cursor is excluded. This matches Xenocept's shipped behavior and avoids cursor-position races.
- One overlay is used for the single monitor under the pointer; there is no virtual-desktop/all-monitor image.
- Save, copy, and confirm are terminal only after their operation succeeds. Confirm sends only; it does not implicitly save or copy.
- The toolbar is positioned below the selection when it fits, otherwise above it, then clamped to the overlay viewport.
- Direct layer selection/manipulation may reuse Plain's existing behavior, but it is not allowed to add toolbar controls absent from the issue mockup.
- No precision loupe is included. It is neither pictured nor specified and Xenocept has no implementation to reuse.

Two owner-level policy choices remain open. The recommended defaults are safe enough to implement unless the owner overrides them:

1. **Global shortcut without an eligible chat target:** allow capture, save, and copy; disable confirm with a localized “Open a chat to send” explanation. This preserves the usefulness of a global capture shortcut without guessing a recipient.
2. **Wayland fallback:** require the full Plain annotation overlay, but permit the desktop portal's screen chooser before it when direct monitor capture is unavailable. Do not silently fall back to an unsafe or blank XWayland capture.

## Baseline evidence

### Repository state

- The branch was created from freshly fetched upstream `main` at `52531249`.
- Upstream has no open pull requests as of the baseline.
- Recent commits changed `ChatInput.vue`, `main.ts`, `src-tauri/src/commands/window.rs`, and `src-tauri/tauri.conf.json`. Those are hot integration files and remain serialized under one integration owner.
- The latest file-drop fix deliberately adds `dragDropEnabled: false`; capture work must preserve it.
- Plain Desktop currently contains about 8,278 lines under `src/views/image-editor/` and already defines the requested layer types, mosaic renderer, Pixi display renderer, Yjs document/undo behavior, transform logic, and export compositing.
- Xenocept was audited at `35efe0eab84ba8bf26e1fc575b2db40302685bb7`. Its relevant implementation is roughly 29,000 lines, but most of that surface is product-specific and intentionally excluded.

### Baseline commands

| Gate | Baseline result |
|---|---|
| `corepack yarn typecheck` | Pass; 16.07 s |
| `env VITE_APP_MODE=tauri corepack yarn build` | Pass; 5.78 s |
| `cargo +1.96.0 test --manifest-path src-tauri/Cargo.toml --lib` | Pass; 143 tests |
| `corepack yarn test` | 499 passed, 52 skipped, 4 failed |
| Isolated cross-window rerun | Pass; 5 tests |
| `cargo +1.96.0 test --locked ...` | Baseline failure: moving `plain-rs` Git dependency makes the checked-in lock stale |

The persistent frontend baseline debt is capped at exactly these three test IDs:

- `tests/lib/window-client.test.ts > getActiveClientId falls back to the desktop clientId when nothing is bound`
- `tests/lib/window-client.test.ts > clearRemoteClientId drops the window back to local mode`
- `tests/lib/local-mode.test.ts > isLocalModeAllowed() is false in web builds even when no device is bound`

`tests/lib/cross-window-store.test.ts > publishes only the declared syncKeys` failed in the concurrent full run and passed alone. It is classified as a pre-existing contention flake, not allowlisted: any future failure must pass an isolated rerun before a phase lands.

The failure cap may shrink but may not grow. Because this feature changes Rust dependencies, P0 must regenerate and commit a reproducible `Cargo.lock`; `cargo +1.96.0 test --locked` becomes mandatory after P0.

## Territory map

### Plain Desktop seams

| Territory | Existing role | Planned treatment |
|---|---|---|
| `src/views/chat/ChatInput.vue` | Image/folder composer actions and `send-images: File[]` emit | Add only the Tauri-gated trigger; async target ownership remains in `ChatView` |
| `src/views/chat/ChatView.vue` and `hooks/chat-upload.ts` | Existing optimistic image upload/send path | Reuse the upload protocol, but snapshot immutable destination IDs before the first await and expose an acknowledged async capture consumer |
| `src/views/image-editor/utils/types.ts` | Typed annotation-layer union | Reuse as the canonical layer contract |
| `renderer.ts`, draw-layer utilities, Pixi renderer | Preview and final compositing, including mosaic | Reuse behind an extracted in-memory annotation session |
| `useImageEditorDoc*`, `useImageEditorUndo`, `useImageEditorLayers`, transform utilities | Typed mutations and undo/redo | Reuse; characterize before extraction |
| `useImageEditorCore.ts` | Currently mixes editor engine, persistence, transport, routing, and lifecycle | Split once; full editor keeps adapters, capture uses ephemeral adapter |
| `useImageEditorPersistence.ts`, `plain-app-store.ts`, `event-sync-transport.ts` | Phone/server project storage and Yjs transport | Explicitly forbidden from capture imports |
| `src/plugins/router.ts`, `src/main.ts`, `App.vue` | SPA and utility-window bootstrap | Dispatch `/screen-capture` before full-app imports; do not add it to the shared router or boot `App.vue` |
| `src-tauri/src/lib.rs`, `commands/mod.rs` | Plugin/state/command registration and process window lifecycle | Register a modular capture subsystem; keep orchestration out of `lib.rs` |
| `commands/window.rs` | General 1200×800 cascaded app windows | Do not reuse for the capture overlay; it has incompatible semantics |
| `capabilities/*.json` | Per-window Tauri plugin/core permissions | Give the overlay only required plugin/core permissions; validate every custom command's caller label and session ownership, and record the existing process-wide custom-command ACL debt |
| `.github/workflows/check.yml`, `release.yml` | Linux/native dependency installation and cross-platform builds | Add only dependencies proved by P0 |

Non-obvious consumers and lifecycle hazards:

- Chat routes are kept alive by full path. A global completion event consumed by all cached `ChatInput` instances can send to the wrong conversation.
- Window identity is per webview. The origin label and active chat target must be frozen at capture start.
- Windows/Linux exit when no visible app window remains. The origin must be restored before the last overlay is destroyed.
- macOS dock-window enumeration must exclude capture utility windows.
- Every utility webview currently boots prefs, proxy ports, login peers, app sockets, discovery, and media-preview prewarming. The capture route must not pay for or start those services.
- Tauri events serialize JSON and are unsuitable for 30–130 MiB raw monitor frames.

### Xenocept reuse ledger

| Xenocept territory | Decision | Reason / required correction |
|---|---|---|
| `src/platform/mod.rs` capture trait and monitor model | Adapt | Good ownership seam; replace ambiguous region coordinates with explicit physical-pixel contracts |
| `src/platform/windows.rs` | Adapt | Proven `xcap` monitor capture and Win32 cursor location |
| `src/platform/macos.rs` | Adapt | Proven permission preflight, direct CoreGraphics capture, stride-aware BGRA→RGBA conversion |
| `src/platform/x11.rs` | Adapt selectively | Reuse singleton X display, XShm fast path, and fallback; re-enumerate on each session to survive hotplug |
| `src/platform/portal_shortcuts.rs` | Adapt | Needed because the ordinary global-hotkey backend is X11-only on Linux |
| `trigger_overlay_capture` and overlay positioning in `src/main.rs` | Adapt as small coordinator modules | Reuse cursor-monitor selection, borderless platform positioning, focus workarounds, and stale-capture protection |
| Raw RGBA handoff and asynchronous PNG fallback | Adapt to typed Tauri binary IPC | Preserve speed; remove localhost HTTP and `eval` callbacks |
| Initial-click/native-pointer recovery | Port only if P0 reproduces the lost-pointer problem | Valuable workaround, but avoid platform code with no demonstrated Plain failure |
| Four-canvas annotation model and 5,201-line canvas | Omit | Plain already owns a typed, tested-at-the-boundaries editor architecture |
| Radial menu, comments, notes, bubbles, emoji, blur, screenshot stack | Omit | Not in issue #19; mosaic already exists in Plain |
| AeorDB, photo roll, sessions, client HTTP API, plugins, auto-send | Omit | Explicitly prohibited or architecturally foreign |
| Fire-and-forget submit/reset flow | Reject | It destroys user work before delivery success is known |

Xenocept documentation is evidence, not a contract. Its vision and current implementation disagree about continuous warm captures, all-monitor capture, eraser support, undo snapshots, session-area persistence, and Wayland/PipeWire support. Code-level observations and issue #19 take precedence.

## Architecture and named contracts

### Ownership split

```text
ChatInput / global shortcut
          |
          v
CaptureClient (small target/session IDs only)
          |
          v
Rust CaptureCoordinator ----> ScreenCaptureBackend (per OS)
          |                              |
          |                              v
          |                       physical RGBA frame
          v
borderless /screen-capture overlay
          |
          v
ephemeral Plain AnnotationSession + SelectionModel
          |
          v
PNG result --binary IPC--> frozen CaptureTarget --send-images--> existing upload path
```

Rust owns permissions, monitor enumeration/capture, cursor-monitor choice, overlay windows, hotkeys, sensitive byte buffers, native clipboard/save, and restoration. Vue owns selection, annotation interaction, compositing, and localized feedback. Neither side persists capture sessions.

### Domain contracts

The implementation must name and type these contracts before moving code:

- `PhysicalPoint` and `PhysicalRect`: signed global origin, unsigned size, always physical desktop pixels.
- `FramePoint` and `FrameRect`: monitor-local coordinates in the captured image's physical pixels.
- `CssPoint` and `CssRect`: logical coordinates inside the overlay webview.
- `MonitorGeometry`: stable session-local ID, physical origin/size, logical origin/size, and scale factor.
- `CapturedFrame`: session ID, monitor geometry, checked width/height/stride/pixel format, and a bounded raw byte handle.
- `CaptureTrigger`: composer or global.
- `CaptureOrigin`: optional invoking Plain webview and its pre-capture visibility/minimized/focus state. A global shortcut may have no origin to hide.
- `CaptureTarget`: eligible webview label plus opaque frontend target token. It is independent of the origin; Rust must not know chat recipients.
- `NativeCapturePhase`: `idle → waiting-for-overlay → hiding-origin → capturing → frame-available → awaiting-presentation → active → result-available → delivering → restoring → idle`, with error/cancel edges through `restoring`.
- `CaptureUiPhase`: `waiting → loading-frame → selecting → selected ↔ drawing|resizing|moving|editing-text → exporting → waiting-for-delivery → completed`.
- `CaptureResult`: session ID, PNG byte handle, dimensions, filename, and MIME; never the PNG byte array in a JSON event.
- `CaptureError`: stable machine code plus optional diagnostic cause. User messages are localized in Vue.
- `SelectionModel`: normalized/clamped frame-space rectangle plus active drag/resize operation.
- `AnnotationSession`: in-memory document, layer binding, undo manager, renderer, pointer state, source image, and export function; persistence and transport are injected adapters, not unconditional imports.
- `CaptureExport`: `save`, `copy`, or `send`, each returning success before cleanup.

Stateless coordinate conversion, geometry, toolbar placement, frame validation, and PNG naming remain pure functions. Stateful native session/window ownership and annotation documents may be classes/managed state.

### Coordinate invariant

`FrameRect` is the only canonical selection/export coordinate space. Conversion happens in one module:

```text
frame_x = (css_x - image_css_left) * frame_width / image_css_width
frame_y = (css_y - image_css_top)  * frame_height / image_css_height
```

Every multiplication/addition used to size native buffers is checked. Negative desktop origins never enter frame-local crop arithmetic. A selection is clamped before conversion to integer export bounds. Annotation layers also remain in frame pixels, so overlay DPR changes and window movement cannot change the PNG.

### Binary handoff invariant

- JSON events carry only session/result IDs and metadata.
- The overlay retrieves raw RGBA through a Tauri raw IPC response (`ArrayBuffer`) and acknowledges successful canvas decode; Rust then drops its raw frame.
- The overlay sends the final PNG through a raw IPC request. Rust stores one size-bounded result under an authenticated result ID.
- The frozen target acquires one authenticated delivery-attempt lease to read the PNG through a raw IPC response. Concurrent reads are rejected; an explicit failure releases the lease without clearing bytes, and only a success acknowledgment clears the result.
- The target creates `File([bytes], filename, { type: 'image/png' })`, validates the session/target token, invokes the registered async `ChatView` consumer, and acknowledges delivery only after the existing upload path resolves.
- Results expire on acknowledgment, cancel, terminal error, target close, or timeout. No temporary file is the primary transport.

P0 must prove this raw-IPC path with a 4K deterministic frame. If a platform blocks it, the only fallback is a random session-scoped mode-0600 temp artifact with verified cleanup; base64/JSON is not allowed.

### Capture window invariant

- The overlay is prewarmed hidden after ordinary startup, analogous to the media-preview warm window, but it uses a minimal bootstrap and starts no Plain business services.
- It is borderless, opaque, always on top, excluded from task switchers/dock menus where supported, and sized to one monitor.
- macOS uses a borderless monitor-sized window, not native fullscreen, so it does not create a separate Space or capture a black desktop.
- Linux uses GTK monitor targeting on the main thread; Windows uses explicit physical placement/size before fullscreen presentation.
- The overlay remains hidden until the frame has decoded. A user must never see a live or partially loaded desktop behind the selection layer.
- When Plain is the foreground trigger, only the invoking/target Plain window is hidden before capture. Its prior visibility, minimized state, and focus are restored on every terminal edge.
- Restoration happens before overlay destruction to avoid Plain's last-visible-window exit rule.

### Shortcut invariant

- Windows/macOS/X11 register the process-level shortcut in Rust through the official Tauri global-shortcut plugin.
- Wayland uses the XDG GlobalShortcuts portal adapter derived from Xenocept. Registration denial or collision is surfaced; it is never silently treated as success.
- The fixed issue-defined defaults are used in this PR. A configurable shortcut preference is roadmap work.
- The shortcut and composer button call the same `CaptureCoordinator::start` path.

### Selection and annotation invariant

- The source frame is immutable for the session.
- Selection changes define a non-destructive output boundary; they never rescale or translate existing annotations.
- The outside dimmer, selection border, dimensions, handles, and toolbar are transient and never rendered into the PNG.
- One completed stroke/shape/arrow/text/mosaic operation and each committed layer transform produce one undo item. Pointer previews and no-op clicks do not.
- Final composition is source frame + visible annotation layers, clipped to `FrameRect`, exported at exact frame-pixel dimensions.
- Text editing consumes Enter/Escape before capture-level shortcuts. Export never occurs from a key event handled by an input/textarea.

### Privacy and failure invariant

- At most one raw frame and one final result are held by Rust. Raw frame size is capped at 256 MiB after checked dimension arithmetic.
- No pixel bytes, clipboard contents, file paths, or recipient identifiers are logged.
- Cancel, timeout, permission denial, monitor removal, decode/encode error, overlay crash, origin close, and app exit clear native and frontend buffers and restore windows.
- Copy/save/send failures preserve the live overlay and annotation state for retry.
- DRM/protected-content black frames are reported as a platform limitation; the application does not attempt to bypass protection.

## Phased implementation

Each phase is one reviewable commit/revert unit and must land green before the next dependent phase. Upstream `main` is fetched before every phase; hot-file conflicts are resolved by hand with union semantics, followed by typecheck and the phase gate. Nothing is pushed after a failed gate.

### P0 — Dependency, platform, and IPC proof

Owner: architecture/native lead only.

Work:

- Re-audit current upstream and record any drift from `52531249`.
- Add typed contracts and an injectable `ScreenCaptureBackend` skeleton without product UI.
- Spike the latest stable `xcap` rather than copying Xenocept's older pin; verify its transitive Git dependencies and license compatibility.
- Prove native capture and physical/logical monitor metadata on this KDE host, Windows CI/host, and macOS CI/host. Exercise X11 and Wayland separately.
- Prove macOS permission denied/granted behavior and signed-package metadata.
- Prove raw binary IPC round-trip with deterministic 1080p and 4K RGBA fixtures.
- Prove native image clipboard writes and native save-dialog access in packaged apps. If the Tauri clipboard plugin is unstable on macOS, isolate a platform adapter rather than leaking a workaround into Vue.
- Add the exact Linux native packages required by the selected backend to check and release workflows.
- Regenerate `Cargo.lock` and make `--locked` tests reproducible.
- Create `docs/issues/19/progress/native.md` with dependency versions, measured latencies, platform results, and the Xenocept provenance ledger.

Gate:

- Rust fake-backend tests cover no monitors, cursor outside all monitors, negative origins, permission denial, capture failure, dimension overflow, maximum size, concurrent/repeated capture rejection, stale capture IDs, and hotplug re-enumeration.
- Platform conversion tests cover Windows, X11, and macOS stride/format/coordinate behavior.
- Raw IPC fixture checksum and byte length agree at 1080p and 4K.
- `cargo +1.96.0 test --locked --manifest-path src-tauri/Cargo.toml --lib`
- `cargo +1.96.0 check --locked --manifest-path src-tauri/Cargo.toml`
- Cross-platform check workflow green.

Rollback: remove the new isolated module/dependencies/workflow packages; no frontend behavior exists yet.

### P1 — Characterize and extract Plain's annotation kernel

Owner: one frontend editor lead. Do not parallelize edits inside `useImageEditorCore.ts`.

Work:

- Add deterministic characterization fixtures before moving editor behavior.
- Extract a reusable in-memory `AnnotationSession` from `useImageEditorCore.ts`.
- Keep project persistence, URL/history mutation, PlainApp GraphQL storage, and `EventSyncTransport` in the existing full-editor adapter.
- Keep large screenshot sources out of Yjs/base64; the session accepts an already decoded image/canvas source.
- Expose explicit preview render and `renderPng(selection)` operations with error results.
- Preserve the existing image editor's tools, project restoration, autosave, synchronization, and export behavior.
- Create `docs/issues/19/progress/annotation-core.md` with before/after output hashes and moved ownership.

Gate:

- Old-vs-old deterministic noise floor is measured first. Geometry/layer JSON and non-text canvas output must be exact-zero diff. Text is compared semantically because OS font rasterization is platform-dependent.
- Before-vs-after full-editor fixture output has no unexplained differences. Any beneficial divergence is documented and requires owner acceptance rather than being silently shipped.
- Tests cover each requested layer creation path, one undo item per gesture, redo invalidation, transform commits, mosaic effect, crop/export dimensions, transparent pixels, layer order, and `toBlob(null)` failure.
- Capture-mode isolation test asserts zero GraphQL calls, zero transport connections/broadcasts, zero history mutations, and zero persistence writes.
- Existing image-editor store/sync tests stay green.
- `corepack yarn typecheck`
- Focused image-editor/browser tests green.
- Full `corepack yarn test` has no failures beyond the exact baseline cap; any cross-window failure passes in isolation.
- `env VITE_APP_MODE=tauri corepack yarn build`

Rollback: revert the extraction commit; no capture consumer exists yet.

### P2 — Native capture coordinator and overlay lifecycle

Owner: native lead. May run in parallel with P1 only after P0 contracts are frozen. The integration owner lands the small bootstrap-dispatcher slice and hands the ready shell back to the native lead for barrier tests.

Planned ownership:

- `src-tauri/src/commands/screen_capture/**`
- capture-specific additions to `commands/mod.rs`
- capture capability/permission files
- capture-specific dependency and workflow changes
- integration-owned `src/main.ts`, `src/bootstrap/full-app.ts`, and ready-only `src/views/screen-capture/**` shell as one serialized handoff

Work:

- Implement managed `CaptureCoordinator` state and per-platform backend adapters.
- Adapt Xenocept's cursor-monitor selection, macOS permission/conversion code, X11 XShm/singleton-display behavior, Wayland shortcut portal, borderless overlay positioning, focus fixes, and stale-capture protection.
- Create/prewarm a dedicated `screen-capture-overlay` webview at `/screen-capture` and land its ready-only minimal bootstrap shell in this phase so the native readiness barrier can be proved.
- Split `src/main.ts` into a dependency-free path dispatcher and dynamically imported full-app/capture bootstraps. The capture branch must never import `App.vue`, the shared router, Pinia, i18n preferences, sockets, discovery, GraphQL, or media-preview initialization.
- Implement a listener-before-ready generation barrier, a separate frame-presented barrier, one-shot raw-frame retrieval, single-reader result leases, explicit retry release, and result acknowledgment.
- Implement idempotent start/cancel/fail/complete/ack cleanup.
- Record and restore the origin window state; exclude utility windows from lifecycle/dock enumeration.
- Register process-level shortcuts and expose registration status.

Gate:

- State-machine tests cover every legal edge and reject every illegal/re-entrant edge.
- Window-adapter tests prove origin restoration before overlay destruction on success, cancel, decode failure, overlay close, and origin close.
- Stale/wrong-session/raw-result reads fail closed; frame reads are one-shot, while a result permits only one delivery-attempt lease at a time and survives explicit failure release until acknowledgment clears it exactly once.
- Start-before-ready, ready-before-start, stale generations, readiness timeout, and overlay reload/destruction restore safely; the overlay is never shown before frame presentation is acknowledged.
- Events contain metadata only; a test rejects payload contracts containing byte arrays/base64.
- Shortcut mapping tests cover `Alt+A` and `Option+Command+A`; Wayland portal translation tests are retained/adapted.
- Rust full `--locked` test/check gates and cross-platform CI green.

Rollback: revert the coordinator commit; the editor extraction remains behavior-identical and useful independently.

### P3 — Frozen overlay, selection, and capture toolbar

Owner: capture-frontend lead. Extend the P2 capture shell within `src/views/screen-capture/**`; own capture-specific tests/i18n only.

Work:

- Extend the guarded capture-only bootstrap shell; `/screen-capture` remains outside the shared router and full-app dependency graph.
- Fetch/decode the native frame, acknowledge it, and render a fully opaque frozen monitor.
- Implement the pure `SelectionModel`: four drag directions, min size, bounds clamp, move, eight resize handles, live frame-pixel dimensions, exterior dim, and toolbar placement.
- Compose the P1 `AnnotationSession` with capture-only toolbar/tool policy.
- Implement rectangle, ellipse, arrow, pen, text, mosaic, undo/redo, color, and stroke-width interactions.
- Implement keyboard priority and localized retryable errors.
- Add capture strings to all 17 locale modules, reusing existing common/image-editor translations when semantics match.
- Use a deterministic injected frame fixture so almost all frontend behavior is testable without a real desktop capture.
- Create `docs/issues/19/progress/overlay.md` with screenshots and browser-test evidence.

Gate:

- Pure geometry tests cover reverse drags, zero/min selection, all edges/corners, handle crossing, interior movement, viewport clamping, negative monitor origins, DPR 1/1.25/1.5/2, and toolbar edge flips.
- Browser tests cover every pictured tool, pointer cancel, active text editing, keyboard priority, undo/redo, selection changes after annotation, and exact clipped PNG dimensions.
- Pixel tests use a deterministic non-text frame and prove shape/arrow/pen/mosaic composition, clipping, alpha, and layer order. Text tests compare layer semantics and bounds rather than OS-specific glyph pixels.
- Overlay tests prove no auth, socket, discovery, proxy, media-preview prewarm, GraphQL, persistence, or network bootstrap occurs.
- Direct `/screen-capture` use outside a valid Tauri session fails closed.
- P1 frontend gates remain green.

Rollback: remove the route/view; native coordinator remains unreachable except through test commands.

### P4 — Chat target delivery and shared trigger integration

Owner: integration lead only. This phase serializes all hot files.

Work:

- Add one `CaptureClient` singleton per webview. `ChatView` registers/activates the currently eligible async consumer, while `ChatInput` only requests capture; freeze its opaque target token per session.
- Install the result listener before invoking start to prevent result-before-listener races.
- Add the Tauri-only scissors action after image/folder in current live-main `ChatInput.vue`.
- Wire confirm to read the retained PNG, construct exactly one PNG `File`, and await the registered `ChatView` consumer before acknowledging native delivery. Do not rely on awaiting a Vue emit, because emits return `void`.
- Snapshot `{ chatId, channelId, appDir }` at upload entry before image decoding or any other await so route changes cannot redirect a captured result.
- Route the global shortcut to the same coordinator/client. Confirm is disabled if no target was frozen; save/copy remain available.
- If the target deactivates or closes, invalidate it and keep the overlay open with save/copy/cancel available rather than sending to a new/current chat.
- Preserve the recent emoji and Tauri file-drop behavior in `ChatInput.vue` and `main.ts`.

Gate:

- Web build: scissors absent and no Tauri capture module evaluated.
- Tauri component test: scissors present in the correct order and duplicate clicks cannot start two sessions.
- Target-correlation tests cover cached ChatInputs, route changes, multiple windows, stale completions, target close, wrong session ID, and global activation without a target.
- Confirm supplies one `File` with exact name/type/bytes to the existing upload path; cancel/save/copy supply none; delivery failure keeps the result readable and retryable until acknowledgment.
- Existing upload mock proves the unchanged `doUploadImages` path receives the file exactly once.
- Recent emoji autocomplete and file-drop tests remain green by name.
- Full frontend/Rust gates green within the baseline cap.

Rollback: remove the only production trigger; P0–P3 remain inert.

### P5 — Native save/copy and complete error handling

Owner: native + capture-frontend leads with disjoint files; integration owner resolves shared command types.

Work:

- Implement native PNG clipboard and native save dialog/write adapters behind `CaptureExport`.
- Grant only capture-overlay plugin/core permissions required for its lifecycle; perform save/copy inside validated custom commands rather than granting generic filesystem/dialog/clipboard access.
- Keep overlay state until native success is acknowledged.
- Add localized permission, shortcut, capture, decode, encode, clipboard, save, target, and monitor-change failures.
- Verify clipboard ownership survives overlay closure and app focus changes.

Gate:

- Adapter tests cover success, denial, cancellation, invalid path, write failure, clipboard unavailable, and size limit.
- Capability audit shows the overlay cannot call unrelated plugin/core filesystem, shell, network, store, opener, notification, or ordinary window APIs. Every capture custom command rejects a wrong caller label/session; the repository's existing lack of generated ACL for all other custom app commands is documented as residual debt, not misrepresented as solved here.
- Save/copy success produces an exact PNG; failure leaves source, selection, layers, and history unchanged.
- No temp artifact remains after any tested terminal path.
- All prior gates remain green.

Rollback: revert native export adapters; confirm/send path remains independently functional.

### P6 — Packaged platform proof and performance hardening

Owner: integration lead; platform testers may run disjoint hosts.

Required packaged matrix:

- Linux KDE X11 and KDE Wayland on this host; add GNOME Wayland when a host/CI runner is available.
- Windows at 100% and mixed 100%/150% DPI, including a monitor with a negative desktop origin.
- macOS Intel and Apple Silicon packaging, Retina/non-Retina movement, first permission prompt, deny, grant, and regrant.
- Single and multi-monitor, monitor hotplug between sessions, repeated/rapid shortcuts, shortcut collision, overlay focus, initial click-and-drag, clipboard persistence, save, target close, and protected/black content behavior.

Performance gates use a prewarmed overlay and 20 measured runs after 3 warmups:

| Measurement | 1080p target | 4K target |
|---|---:|---:|
| Global trigger while Plain is not foreground → frozen frame ready | p95 ≤ 350 ms | p95 ≤ 700 ms |
| Composer trigger requiring window unmap → frozen frame ready | p95 ≤ 750 ms | p95 ≤ 1,100 ms |
| Pointer event → preview frame | p95 ≤ 16.7 ms | p95 ≤ 16.7 ms |
| Confirm → PNG result available | p95 ≤ 500 ms | p95 ≤ 1,000 ms |

Peak working-set delta is capped at `4.5 × raw RGBA frame bytes + 32 MiB`. Within 10 seconds of close, retained delta must return to within 32 MiB of the prewarmed baseline.

An outlier never passes or blocks on one run. Repeat the suite after host cooldown; if the two p95 values differ by more than 15%, isolate compositor, portal, disk, and code time and report both rather than averaging them away.

Gate:

- Package and smoke-test every release target available in CI.
- Manual matrix evidence is captured in `docs/issues/19/dod-evidence.md` with OS/compositor/DPI/version and screenshots or logs.
- Performance and memory gates pass or any platform-specific exception receives an explicit owner ruling and documented reason.
- No orphan overlay, hidden origin, stuck shortcut, stale result, temp file, or retained capture buffer after 100 sequential sessions.

Rollback: platform-specific fixes remain separated into small commits where practical; the full feature can still be disabled by reverting the P4 trigger commit.

### P7 — Consolidation, deletion, and delivery

Owner: integration lead.

Work:

- Delete spike-only commands, alternate byte transports, transitional editor paths, duplicate geometry/render helpers, and temporary feature flags. “Both paths work” is not an acceptable end state.
- Consolidate progress evidence into:
  - `docs/issues/19/screen-capture-investigation-report.md`
  - `docs/issues/19/dod-evidence.md`
- Document Xenocept provenance, corrections made during adaptation, final dependency/license inventory, architecture, user behavior, platform limitations, and verification results.
- Rebase/merge current upstream by hand, rerun all gates, push only the fork branch, open the linked PR, and post the final report to issue #19.

Mechanical deletion/architecture gates:

```bash
! rg -n "PlainAppProjectStore|EventSyncTransport|useImageEditorPersistence" src/views/screen-capture
! rg -ni "aeordb|xenocept.*plugin|/api/v1/screenshot|base64" src/views/screen-capture src-tauri/src/commands/screen_capture
! rg -n "emit\([^\n]*(bytes|rgba|png)|Vec<u8>.*Serialize" src src-tauri/src/commands/screen_capture
! rg -n "screen-capture-spike|legacyCapture|captureV1" src src-tauri
rg -n "__IS_TAURI__" src/views/chat/ChatInput.vue
```

The first four commands must exit 0 because their inner `rg` finds no banned reference; the final command must find the web-mode guard.

Final gate:

- Typecheck, Tauri frontend build, all focused capture/editor tests, full browser suite within a zero-growth baseline cap, Rust test/check with `--locked`, cross-platform CI, and the packaged matrix all pass.
- `git diff --check` is clean.
- Git status contains only intentional plan/report/code/test changes.
- PR diff contains no PlainApp Android, AeorDB, plugin-host, screenshot-history, or unrelated note/Markdown-editor change.

## Worker briefs

Parallel work begins only after P0 freezes the contracts.

### Native capture worker

- Owns `src-tauri/src/commands/screen_capture/**`, native dependencies, and capture platform tests.
- Must not edit image-editor, chat, router, or locale files.
- Reports: files changed, platform behavior, tests/commands, timings, remaining risks, and provenance entries.

### Annotation-core worker

- Owns image-editor extraction and characterization tests.
- Must not edit Rust, ChatInput, router, main bootstrap, capabilities, or workflows.
- Reports: contracts extracted, before/after hashes, persistence isolation proof, tests/commands, and unexplained divergences.

### Capture-overlay worker

- Extends the P2-owned ready shell in `src/views/screen-capture/**`, then owns that directory, its tests, and capture-specific locale files after P1 interfaces are frozen.
- Must not edit Rust, existing image-editor internals, ChatInput, or global bootstrap files.
- Reports: selection/tool coverage, screenshots, browser tests, accessibility/keyboard results, and errors still needing native support.

### Integration owner

- Sole owner of `ChatInput.vue`, `ChatView.vue`, `main.ts`, router, `App.vue`, Tauri `lib.rs`, general window lifecycle, shared command registration, capabilities, workflows, and final conflict resolution.
- Installs worker commits only after their local gates pass; reruns full gates before every push.
- Maintains `docs/issues/19/dod-evidence.md` and the completion report.

## Test and evidence protocol

### Deterministic fixtures

- Hand-written geometry fixtures cover all signed origins, DPRs, and drag directions.
- Pixel fixtures are deterministic synthetic RGBA patterns with known hashes and contain no private desktop data.
- At least one sanitized frame captured from each real platform is cataloged separately as recorded-from-real evidence; it is not committed if it contains user content.
- Time, UUID, and filename inputs are injected/frozen for output comparisons.

### Equivalence and divergence

1. Run old editor code against itself twice and classify the noise floor.
2. Run old versus extracted editor on the same fixtures.
3. Canonicalize only ordering or generated IDs already proven irrelevant. Do not mask selection or pixel differences.
4. Classify every delta as intended, known noise, regression, or proposed improvement.
5. Fix regressions. Record proposed improvements for owner acceptance before allowing them.
6. Definition of done is zero unexplained differences and zero expected-fail capture specs.

### Per-phase landing ritual

1. Fetch upstream and inspect diffs in every owned/hot file.
2. Apply one phase in its own commit.
3. Run focused tests, typecheck, Tauri build, full browser suite, Rust `--locked` tests/check, architecture grep gates, and `git diff --check` as applicable.
4. If the cross-window contention test fails, rerun it alone once. Any second failure is real and blocks landing.
5. Merge worker work by hand, rerun the full gate, then push. Never chain a push after a failing command.
6. Update the phase progress/evidence document with exact command, result, test count, timing, and platform.

## AGIS adversarial findings

The completed adversarial pass found and absorbed these failure modes:

| Challenged assumption | Finding | Plan response |
|---|---|---|
| “Plain needs Xenocept's editor” | Plain already has a stronger typed annotation stack | Extract one Plain kernel; omit Xenocept canvas/radial UI |
| “Existing Plain screenshot code is the feature” | It captures a phone mirror frame, not the desktop | Keep it out of the implementation territory |
| “Tauri events can carry the image” | JSON/base64 copies can explode memory on 4K/8K frames | Binary IPC, one-shot bounded frame handles, and acknowledged retryable result handles |
| “Global shortcut plugin covers Linux” | Its Linux backend is X11-only | Adapt Xenocept's XDG portal path for Wayland |
| “Xenocept proves Wayland capture” | It has no owned PipeWire capture path and docs overstate support | P0 must prove direct capture or use an explicit portal chooser fallback |
| “Region coordinates are reusable” | Xenocept backends disagree about global versus local coordinates | New typed physical/global/frame/CSS contracts and conversion tests |
| “Close overlay, then restore” | Plain may exit when the last visible window closes | Restore origin before overlay destruction |
| “The active chat can be looked up at completion” | Keep-alive routes and multiple windows can redirect the result | Freeze an opaque target token at session start and fail closed if stale |
| “A normal router route is a lightweight overlay” | Static `main.ts`/router/App imports start preferences, sockets, discovery, chat state, and media preview before route resolution | Branch before all full-app imports and mount a dedicated capture root |
| “Vue emit can acknowledge an async upload” | Component emits return `void`, and the upload hook reads reactive route IDs after async image decode | Register an async `ChatView` consumer and snapshot destination IDs before the first await |
| “One-shot PNG reads are retryable” | Reading would destroy the only result before upload success is known, while unrestricted retries can multiply large buffers | One authenticated read lease at a time, explicit failure release, and explicit success acknowledgment |
| “A capability JSON restricts custom commands” | This app has no generated Tauri `AppManifest`; capability files currently govern plugin/core commands only | Restrict plugin/core permissions, validate capture callers in Rust, and disclose legacy process-wide custom-command ACL debt |
| “Current image editor core is reusable as-is” | It autosaves, broadcasts Yjs, rewrites history, and loads base64 | Extract ephemeral core; mechanically ban persistence/transport imports |
| “Save/copy can clear optimistically” | Xenocept's fire-and-forget submit loses work on failure | Cleanup only after acknowledged native/delivery success |
| “Latest local main is current” | Upstream landed emoji/file-drop changes in the exact hot files | Fresh-main branch and per-phase drift checks |
| “Main is green” | Three stable browser failures, one contention flake, and stale Rust lock exist | Exact non-growing debt cap; isolation protocol; fix lock in P0 |

## Risks and mitigations

| Risk | Impact | Mitigation / blocking evidence |
|---|---|---|
| Wayland capture/shortcut portal differs by compositor | High | KDE X11/Wayland proof in P0; portal fallback is explicit and user-visible |
| macOS capture permission or separate-Space behavior | High | Reuse permission/borderless patterns; test deny/grant/regrant in signed packages |
| Mixed DPI and negative monitor origins | High | Frame-pixel canonical space and exhaustive pure conversion tests |
| Origin remains hidden or app exits | High | Single coordinator cleanup guard and restore-before-destroy state tests |
| Wrong cached chat receives result | High | Immutable target token, per-session correlation, stale-target failure tests |
| Memory spikes on 4K/8K | High | Single-monitor scope, raw-size cap, binary IPC, prompt acknowledgment/drop, memory gate |
| Clipboard plugin instability on macOS | Medium | P0 packaged spike behind a platform adapter; custom main-thread fallback only if reproduced |
| xcap/transitive Git dependency churn | Medium | Latest stable audit, committed lock, `--locked` CI, license inventory |
| Native Linux packages break CI/release | Medium | Update both check and release workflows in P0 and test from clean runners |
| Initial pointer-down is swallowed while overlay focuses | Medium | Reproduce first; selectively port Xenocept native-pointer recovery if needed |
| Capture includes Plain before compositor unmaps it | Medium | Measured platform settle/visibility step and frozen-frame screenshot proof |
| Image-editor extraction collides with upstream work | Medium | Single owner, characterization first, per-phase upstream sync |
| Protected content produces black pixels | Low/unavoidable | Report platform limitation; do not bypass protection |

## Non-goals and roadmap

| Item | Status | Reason / dependency |
|---|---|---|
| Window/object recognition and auto-snapping | Roadmap | Not pictured; requires accessibility/window-enumeration contracts per OS |
| Virtual-desktop or cross-monitor selection | Out of scope | Issue explicitly confines selection to one monitor |
| Configurable capture shortcut | Roadmap | Fixed defaults first; settings UX and conflict migration are separate |
| Capture history/photo roll | Out of scope | Privacy expansion and explicit Xenocept storage feature |
| Persistent/collaborative capture projects | Out of scope | Existing full image editor owns persistence; capture is ephemeral |
| OCR, scrolling capture, delayed capture, cursor toggle | Roadmap | Separate product and platform work |
| Xenocept comments/notes/bubbles/radial menu/plugins | Out of scope | Not requested and would duplicate/expand Plain architecture |
| Android/phone changes | Out of scope | Desktop-only ruling; existing `send-images` contract is sufficient |
| Phone screen-mirror screenshot overhaul | Out of scope | Different capture source and user workflow |
| Precision loupe | Roadmap only if requested | Not in issue mockups and no Xenocept implementation exists |

## Verifiable definition of done

- [ ] Tauri ChatInput displays the scissors action in the specified order; web ChatInput does not.
- [ ] `Alt+A` and `Option+Command+A` reach the same coordinator as the button, with user-visible registration failure.
- [ ] One cursor monitor is frozen, the overlay never shows a live desktop, and selection cannot cross a monitor.
- [ ] Every pictured selection, tool, color, width, history, and action behavior has an automated test.
- [ ] Confirm produces one exact PNG `File`, exercises the existing upload path once against the destination snapshotted before async work, and acknowledges only after success.
- [ ] Save/copy succeed in packaged Windows/macOS/Linux tests; failure is retryable and non-destructive.
- [ ] Capture imports no persistence, GraphQL, Yjs transport, AeorDB, Xenocept plugin, or phone code.
- [ ] Pixel bytes never travel in JSON events, URLs, Pinia, logs, GraphQL, or persistent project state.
- [ ] All session terminal/error paths restore hidden windows and release byte/result handles.
- [ ] Exact deterministic old-vs-new editor comparison has zero unexplained divergence.
- [ ] Rust `test` and `check` pass with `--locked` on all CI OS targets.
- [ ] Frontend typecheck/build/focused tests pass; full browser failures do not exceed the exact baseline cap, which never grows.
- [ ] The packaged OS/compositor/DPI/permission matrix and 100-session soak pass.
- [ ] Performance and memory numbers meet the P6 budgets or have an explicit owner-approved exception.
- [ ] Transitional/spike/alternate paths are deleted and all P7 grep gates pass.
- [ ] Xenocept provenance and dependency/license audit are complete.
- [ ] `screen-capture-investigation-report.md` and `dod-evidence.md` contain reproducible proof.
- [ ] The fork branch is current, the linked PR is open, and the final report is posted to issue #19.
