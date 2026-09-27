# Cross-API Integration Tests

Sends the same GraphQL operations to **both** the plain-web Rust local
server **and** the plain-app Android device server, then asserts the
returned data structures and behaviors match. **plain-app (Android) is
the source of truth** — any Rust-side divergence surfaces as a test
failure.

## Quick start

```bash
# 1. Copy the template and fill in real values
cp .env.test.local.example .env.test.local
# Edit .env.test.local — set RUST_API_URL/TOKEN and ANDROID_API_URL/TOKEN

# 2. Make sure both servers are running:
#    - Rust: `yarn dev:tauri` (or the released desktop app)
#    - Android: open PlainApp, start the HTTP server via QSTileService

# 3. Run the integration tests
yarn test:integration
```

When `.env.test.local` is missing or incomplete, the suites **skip**
themselves (reported as skipped, not failed). The default `yarn test`
command does **not** run integration tests — they're in a separate
vitest project (`integration`).

## Configuration

All config lives in `.env.test.local` (gitignored). See
[.env.test.local.example](../../.env.test.local.example) for the full
template.

| Variable | Required | Description |
|---|---|---|
| `RUST_API_URL` | yes | Rust local server URL (e.g. `http://127.0.0.1:8080`) |
| `RUST_API_TOKEN` | yes | Rust local server URL token (base64 32-byte key, from `app.urlToken`) |
| `RUST_CLIENT_ID` | no | `c-id` header value (default: `integration-test`) |
| `ANDROID_API_URL` | yes | Android device HTTPS URL (e.g. `https://192.168.1.100:8443`) |
| `ANDROID_API_TOKEN` | yes | Android device auth token (base64 32-byte key) |
| `ANDROID_CLIENT_ID` | no | `c-id` header value (default: `integration-test`) |

### How to find the tokens

**Rust local server**: run `yarn dev:tauri`, open the app, and query:
```graphql
query { app { urlToken httpPort } }
```
The URL is `http://127.0.0.1:<httpPort>` and the token is `urlToken`.

**Android device**: open PlainApp, start the HTTP server (QS tile), scan
the QR code. The QR payload contains the URL and token. Alternatively,
after pairing, query the device's `app { urlToken httpsPort }`.

## What the tests cover

Four groups, each in its own file:

### 1. `schema-alignment.test.ts` — read queries
Verifies both servers return the same field set for shared types:
`peers`, `chatChannels`, `chatItems`, `latestChatItems`, `appFiles`.
Includes boundary cases (invalid id, out-of-range offset).

### 2. `mutations.test.ts` — write round-trip
Creates, updates, and deletes channels and chat items on both servers,
asserting the returned struct shape matches. Each test cleans up after
itself via `afterEach`.

### 3. `boundary.test.ts` — error handling
Sends invalid inputs (non-existent ids, empty names, empty content) and
asserts both servers behave consistently: if one errors, the other must
error too. Error *message text* may differ — only the error/no-error
behavior must match.

### 4. `pairing-schema.test.ts` — mutation name alignment
Verifies both servers expose the same pairing mutation names
(`pairDevice`, `cancelPairing`, `respondToPairing`) and accept the same
input type shapes (`PairingDeviceInput`, `PairingRequestInput`). Does
NOT actually trigger pairing — sends empty/placeholder inputs that fail
at schema validation before the resolver runs (no UDP packets sent).

## Known divergences (expected failures)

These tests are **designed to fail** until the Rust server is aligned
with Android. They document real bugs that prevent the web client from
working against the Rust local server:

| Test | Divergence | Fix |
|---|---|---|
| `peers: both return arrays with identical Peer field set (incl. online)` | Rust `Peer` struct lacks `online` field; Android has it via `PeerStatusManager.isOnline` | Add `online: bool` to Rust `Peer` ([types.rs](../../plain-rs/src/api/schema/types.rs)) |
| `cancelPairing mutation name accepted by both` | Rust exposes `cancelPairDevice`; Android (and the frontend) uses `cancelPairing` | Rename in [pairing.rs](../../plain-rs/src/api/schema/pairing.rs) |
| `respondToPairing mutation name accepted by both` | Rust exposes `respondPairDevice`; Android (and the frontend) uses `respondToPairing` | Rename in [pairing.rs](../../plain-rs/src/api/schema/pairing.rs) |

Once fixed, these tests pass and serve as regression guards.

## Architecture

```
tests/integration/
├── helpers.ts                    # Encryption, fetch client, shape comparison
├── setup.ts                      # Loads .env.test.local, disables TLS check
├── schema-alignment.test.ts      # Group 1: read queries
├── mutations.test.ts             # Group 2: write round-trip
├── boundary.test.ts              # Group 3: error handling
└── pairing-schema.test.ts        # Group 4: pairing mutation names
```

The test client mirrors the wire protocol used by
[`src/lib/api/gql-client.ts`](../../src/lib/api/gql-client.ts):
XChaCha20-Poly1305 encryption with replay-protection wrapping
(`"TIMESTAMP|NONCE|JSON"`). The same encrypted payload is sent to both
endpoints; responses are decrypted and compared structurally.

### Vitest project

Defined in [`vite.config.ts`](../../vite.config.ts) as the `integration`
project: Node environment, single-threaded, separate from the `unit`
(Chromium) and `cws` (Node cross-window-store) projects. The
`NODE_TLS_REJECT_UNAUTHORIZED=0` setting in `setup.ts` only affects this
project's worker thread — it does not leak into other projects.

## Troubleshooting

**"Both endpoints configured" but tests skip anyway**: check that
`.env.test.local` is in the `plain-web/` root (same directory as
`package.json`). The setup file uses `process.loadEnvFile('.env.test.local')`
with a relative path.

**`unauthorized (401)` errors**: the token is wrong or expired. Re-fetch
it from the `app.urlToken` GraphQL field on each server.

**`desktop_access_disabled (403)` on Android**: the device's web access
toggle is off. Enable it in PlainApp's settings.

**Connection timeout to Android**: ensure the device is on the same
network, the HTTPS port is open, and no firewall blocks the port. The
self-signed cert is handled automatically (TLS rejection disabled).

**`failed to parse response` errors**: the token doesn't match the
server's encryption key. The token must be the exact base64-encoded
32-byte key the server uses. Verify by checking the `app.urlToken`
field.
