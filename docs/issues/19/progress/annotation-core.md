# P1 annotation-core progress

## Entry gate

- Owner: frontend annotation-core worker.
- Entry commit: `109b1de9390333a2cdae87ad027f5f05f7105dc9`.
- Scope: characterization, extraction of the reusable annotation session, and the existing full-editor compatibility adapter.
- Owned files:
  - `src/views/image-editor/**`
  - `tests/views/image-editor/**`
  - this progress ledger
- Forbidden files: Rust/Tauri capture work, chat/router/bootstrap integration, workflows, manifests, and lockfiles.
- Concurrent P0 changes were already present in the worktree and were left untouched.

## Characterized behavior

- `useImageEditorCore()` retains the public groups and nested keys consumed by `ImageEditor.vue`.
- Initial tool state is `select`, color `#ef4444`, line width `4`, font size `48`, and canvas size `1920x1080`.
- Pointer gestures create the issue-requested rectangle, ellipse, arrow, freehand, text, and mosaic layer semantics with fixed coordinates, selected color, and selected width.
- The non-text render fixture uses a deterministic 64x48 opaque source plus mosaic, rectangle, ellipse, arrow, and freehand layers in a fixed order.
- Rendering the fixture twice produces an exact-zero byte diff in Chromium.
- Baseline SHA-256 over the final RGBA bytes is:
  `c1726456f8959f8d905190b73002a119ca99daf3a89cb42ca21625236536effe`.
- Text is intentionally characterized semantically rather than by raster hash because font rasterization is platform-dependent.

## Extracted boundary

- `useAnnotationSession()` now owns the in-memory document/binding, Pixi preview, tools, pointer state, layer operations, gesture-scoped undo, decoded raster source, clipped PNG export, and explicit disposal.
- Capture-safe defaults disable source movement and destructive crop, keep text as a draft until explicit commit, and install no persistence, URL, router, GraphQL, transport, eventbus, or store adapter.
- `source.set({ drawable, width, height, release? })` accepts an already-decoded `HTMLCanvasElement` or `HTMLImageElement`, validates exact raster dimensions, never serializes the source into Yjs, and transfers release ownership to the session.
- `render.preview()` is a synchronous readiness probe/schedule operation. `await render.awaitPaint()` waits for the scheduled main render to flush and then invokes Pixi's synchronous render before resolving. Both return explicit `disposed` / `preview-not-ready` results.
- `text.begin/update/commit/cancel` keeps capture text outside the document and export until a non-empty commit. Cancel and empty commit leave no layer or undo item.
- `exportOps.renderPng(selection?)` returns explicit validation/context/encode errors, excludes the visual transparency grid, clips without moving layers, and produces the exact requested pixel dimensions.
- `dispose()` is idempotent, stops Vue watches/RAF work, safely invalidates an in-flight Pixi initialization, resolves pending paint barriers, destroys Yjs/Pixi resources, clears decoded raster/text refs, and invokes the current source release callback exactly once even if that callback throws.
- `useImageEditorCore()` is now the full-editor adapter. PlainApp persistence, URL/history mutation, GraphQL-backed storage, autosave, and `EventSyncTransport` remain there; it opts into legacy source movement, destructive crop, and immediate default-text insertion.
- Shared undo retains the full editor's zero-timeout behavior for ordinary mutations. Only active pointer move/resize/rotate gestures temporarily group their transactions; pointer up commits one undo item, while capture pointer-cancel rolls the group back and restores any pre-existing redo history.

`pointer.onPointerCancel()` discards uncommitted shape/text previews and restores the pre-gesture layer/source value for active moves or transforms without retaining a new undo/redo entry. The existing full editor continues to route DOM pointer-cancel through its legacy pointer-up handler; this capture-only API does not change that behavior.

## Evidence

Baseline before adding characterization:

```text
corepack yarn vitest run --project unit \
  tests/views/image-editor/plain-app-store.test.ts \
  tests/views/image-editor/sync-protocol.test.ts

2 files passed; 11 tests passed; duration 1.32 s.
```

Failing-first digest capture:

```text
corepack yarn vitest run --project unit \
  tests/views/image-editor/annotation-core-characterization.test.ts

3 tests passed; 1 failed only because the expected digest was the explicit
__CHARACTERIZATION_HASH__ placeholder. Chromium reported c1726456...36effe.
```

Focused green gate after recording that independent baseline:

```text
corepack yarn vitest run --project unit \
  tests/views/image-editor/annotation-core-characterization.test.ts \
  tests/views/image-editor/plain-app-store.test.ts \
  tests/views/image-editor/sync-protocol.test.ts

3 files passed; 15 tests passed; duration 1.62 s.
```

Vitest printed its existing `test.poolOptions` deprecation warning; it did not affect the result.

Failing-first extraction proof:

```text
- The first boundary run failed because useAnnotationSession.ts did not exist.
- Three adversarial lifecycle tests then failed against the initial extraction:
  preview reattachment incorrectly invalidated an in-flight export, decoded
  dimensions were not checked, and a throwing release hook interrupted cleanup.
- A shared-Infinity undo regression test failed because two unrelated document
  mutations collapsed into one item.
- Paint-barrier and draft-text tests failed before those APIs were introduced.
- Move/transform pointer-cancel initially retained the changed layer and a new
  undo item before capture-only gesture rollback was added.
- Pixi initialization/disposal characterization failed with a null-app race
  before the generation-safe renderer lifecycle was implemented.
```

After extraction, the same non-text fixture still hashes to:
`c1726456f8959f8d905190b73002a119ca99daf3a89cb42ca21625236536effe`.

Final focused gate:

```text
corepack yarn vitest run --project unit \
  tests/views/image-editor/annotation-session.test.ts \
  tests/views/image-editor/pixi-renderer-lifecycle.test.ts \
  tests/views/image-editor/annotation-core-characterization.test.ts \
  tests/views/image-editor/plain-app-store.test.ts \
  tests/views/image-editor/sync-protocol.test.ts

5 files passed; 30 tests passed; duration 2.44 s.
```

```text
corepack yarn typecheck

Pass; no output.
```

```text
corepack yarn eslint <all changed P1 TypeScript/Vue/test files>

Pass; no output.
```

```text
corepack yarn prettier --check \
  src/views/image-editor/composables/useAnnotationSession.ts \
  src/views/image-editor/composables/useImageEditorCore.ts \
  src/views/image-editor/composables/useImageEditorExport.ts \
  tests/views/image-editor/annotation-characterization.fixture.ts \
  tests/views/image-editor/annotation-core-characterization.test.ts \
  tests/views/image-editor/annotation-session.test.ts \
  tests/views/image-editor/pixi-renderer-lifecycle.test.ts \
  docs/issues/19/progress/annotation-core.md

Pass; all matched files use Prettier code style.
```

```text
env VITE_APP_MODE=tauri corepack yarn build

Pass; 2,536 modules transformed; production bundle built in 3.27 s.
```

Changed-test lint:

```text
corepack yarn eslint \
  tests/views/image-editor/annotation-characterization.fixture.ts \
  tests/views/image-editor/annotation-core-characterization.test.ts

Pass; no output.
```

Formatting check:

```text
corepack yarn prettier --check \
  tests/views/image-editor/annotation-characterization.fixture.ts \
  tests/views/image-editor/annotation-core-characterization.test.ts \
  docs/issues/19/progress/annotation-core.md

Pass; all matched files use Prettier code style.
```

## Handoff status

The P1 facade is ready for the P3 overlay to consume. The integration should use `source.set`, capture-mode draft text, `render.awaitPaint`, clipped `renderPng`, and `dispose`; it should not install the full-editor persistence hooks or call the legacy image-loading APIs.
