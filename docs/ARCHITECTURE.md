# plain-web Architecture

> **Purpose**: AI-friendly project map. Read this first to avoid blind searching.

## Quick Facts

| Item | Value |
|------|-------|
| Framework | Vue 3.5 + Composition API |
| Bundler | Vite 5.4 |
| Language | TypeScript 5.8 |
| State | Pinia 3.0 |
| API | Custom fetch-based GraphQL client (`gql-client.ts`) |
| i18n | vue-i18n 11 (per-feature module files) |
| Styling | SCSS (no CSS framework) |
| Encryption | XChaCha20-Poly1305 (`@noble/ciphers`) |
| Package manager | Yarn 4 |
| Dev server | `yarn dev` → localhost:3000 |

## Repository Layout (Cargo workspace)

```
plain-desktop/
├── src/            # Vue 3 frontend (map below) — desktop app and NAS web UI
├── plain-rs/       # Shared Rust core: crypto, utils, mdns, chat, library,
│                   #   media (scan/index/thumbnails) + the api feature
│                   #   (GraphQL schema, executor, axum HTTP/WS router)
├── nas/            # plainnas shell: headless NAS server (mounts, disks,
│                   #   DLNA sender, Samba, device status) built on plain-rs
│                   #   cores; design docs in docs/nas/
└── src-tauri/      # Tauri 2 desktop shell: commands, screen capture,
                    #   UI event hooks; API server and proxy live in plain-rs
```

- One root `Cargo.toml` workspace, one `Cargo.lock`, one `target/`.
- `plain-rs` is the core; `nas` and `src-tauri` are shells over it.
- Both shells use `plain_rs::http_server::main_schemas::build_schema()` and the same GraphQL executor. Main and peer GraphQL modules live under `plain-rs/src/http_server/{main_schemas,peer_schemas}`, matching plain-app. HTTP server models, routes, WebSocket handling, proxy, and runtime are grouped under the same `http_server` boundary. `AppCtx` is the shared resolver state; shell operations are supplied through `ShellHooks`, and the UI reads `app.capabilities` for optional features.
- HTTP authentication is configured by `ServerSettings`: the desktop local token and NAS client sessions use the same request and resolver pipeline. Tauri serves the desktop UI, while the shared router serves the NAS Web build.
- Build/test: `cargo check --workspace`, `cargo test --workspace`
  (single crate: `cargo test -p plainnas` / `-p PlainApp` / `-p plain-rs --all-features`).

## Directory Map

```
src/
├── main.ts                    # App bootstrap
├── App.vue                    # Root component
├── components/
│   ├── base/                  # V-prefixed Material Design primitives
│   │   ├── VModal.vue         # Modal (teleport, focus trap, ESC close)
│   │   ├── VTextField.vue     # Text input
│   │   ├── VSelect.vue        # Select dropdown
│   │   ├── VCheckbox.vue      # Checkbox
│   │   ├── VDropdown.vue      # Dropdown menu
│   │   ├── VCircularProgress  # Loading spinner
│   │   ├── VFilledButton.vue  # Primary action button
│   │   ├── VOutlinedButton    # Secondary action button
│   │   ├── VIconButton.vue    # Icon-only button
│   │   └── ...                # VChipSet, VFilterChip, VInputChip
│   ├── {feature}/             # Feature-specific components
│   │   ├── chat/              # Chat components
│   │   ├── files/             # File browser components
│   │   ├── notes/             # Notes editor components
│   │   ├── audio/             # Audio player components
│   │   ├── images/            # Image gallery components
│   │   ├── videos/            # Video gallery components
│   │   ├── messages/          # SMS/MMS components
│   │   ├── contacts/          # Contact components
│   │   ├── calls/             # Call log components
│   │   ├── apps/              # App management components
│   │   ├── bookmark/          # Bookmark components
│   │   └── contextmenu/       # Context menu system
│   └── *.vue                  # Shared components (modals, toolbar, sidebar)
│
├── views/                     # Route-level page components
│   ├── HomeView.vue           # Dashboard
│   ├── LoginView.vue          # Authentication
│   ├── MainView.vue           # Layout shell
│   ├── ScreenMirrorView.vue   # WebRTC screen mirror
│   └── {feature}/             # Feature pages (audios/, chat/, feeds/, etc.)
│
├── hooks/                     # Composable functions
│   ├── chat.ts                # Chat upload task queue
│   ├── chat-route.ts          # Chat route ID decryption
│   ├── chat-data.ts           # Chat peers/channels loading
│   ├── chat-messages.ts       # Chat message CRUD + cache
│   ├── chat-upload.ts         # Chat file/image upload + progress
│   ├── chat-events.ts         # Chat real-time event bus handlers
│   ├── feeds.ts               # RSS subscriptions
│   ├── files.ts               # File operations
│   ├── notes.ts               # Note CRUD
│   ├── tags.ts                # Tagging system
│   ├── search.ts              # Search & filtering
│   ├── media.ts               # Media operations
│   ├── upload.ts              # File upload
│   ├── key-events.ts          # Keyboard shortcuts
│   └── ...                    # audios, contacts, device, sidebar, etc.
│
├── stores/                    # Pinia stores
│   ├── main.ts                # App-wide state
│   ├── files.ts               # File browser state
│   ├── bookmarks.ts           # Bookmarks state
│   └── temp.ts                # Ephemeral state
│
├── lib/                       # Utility modules
│   ├── api/                   # GraphQL queries & mutations (GQL documents)
│   ├── agent/                 # Server agent detection
│   ├── upload/                # Upload queue logic
│   ├── shortcuts/             # Keyboard shortcut definitions
│   ├── search.ts              # Search tokenizer & parser
│   ├── file.ts                # File helpers
│   ├── format.ts              # Formatting (dates, sizes)
│   ├── tag.ts                 # Tag helpers
│   ├── webrtc-client.ts       # WebRTC connection manager
│   └── ...                    # strutil, validator, theme, etc.
│
├── plugins/                   # Vue plugin setup
│   ├── eventbus.ts            # mitt event bus for cross-component events
│   ├── router.ts              # Vue Router routes
│   ├── i18n.ts                # vue-i18n initialization
│   └── ...                    # tooltip, ripple, tapphone, clickaway, etc.
│
├── locales/                   # i18n translations (17 languages)
│   └── en-US/                 # English — per-feature modules
│       ├── index.ts           # Auto-merges siblings via import.meta.glob
│       ├── common.ts          # Generic UI strings
│       ├── chat.ts            # Chat strings
│       ├── files.ts           # File browser strings
│       └── ...                # feeds, media, bookmarks, etc.
│
├── types/                     # TypeScript type definitions
└── styles/                    # Global SCSS styles
```

## Data Flow

```
Vue Component → Composable Hook → gqlFetch() → Local Rust Server (Tauri) / Remote Android Device
                     ↕                    ↕
               Pinia Store          WebSocket event bus (mitt)
```

- **Queries/Mutations**: Via `initQuery()` / `initMutation()` wrappers in `src/lib/api/`
- **Core client**: `gqlFetch()` in `src/lib/api/gql-client.ts` — encrypts with XChaCha20-Poly1305, fetches, decrypts
- **Real-time**: Event-driven updates via mitt event bus (no GraphQL subscriptions)
- **Transport**: native `fetch`/`WebSocket` everywhere; in Tauri builds device URLs are rewritten through the shared local reverse proxy (`plain-rs/src/http_server/proxy/`, see docs/tauri-proxy-strategy.md)
- **State**: Pinia for cross-component state; `ref`/`reactive` for local state

## Build Commands

```bash
yarn dev          # Dev server (port 3000)
yarn build        # Production build → dist/
yarn lint         # ESLint
yarn typecheck    # TypeScript check
```

## Related Docs

- `docs/graphql-client.md` — GraphQL client and transport notes
- `docs/file-upload.md` — upload flow details
- `docs/tauri-proxy-strategy.md` — Tauri proxy performance/stability decisions
- `docs/nas/` — NAS (nas/) design docs: storage, media, DLNA sender, trash, API spec
