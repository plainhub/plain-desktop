# Events (audit log)

PlainNAS keeps a small, bounded event log intended for **security + troubleshooting**. This is a lightweight audit trail for high-level system actions (not a full filesystem activity log).

## Where to view

In the web UI:

- Settings → Events

## Retention

- Stored in PlainNAS runtime data under the configured data directory (see `src/consts.rs`).
- The log is **bounded** (keeps up to the most recent ~1000 events).

## Event types

The log focuses on a small set of high-signal actions:

- `login` / `logout` / `revoke` (session revocation)
- `login_failed` (failed authentication/decryption)
- `mount` / `unmount`
- `mount_failed`
- `format_disk` / `format_disk_failed`

Notes:

- Event messages are intentionally short and should **not** contain secrets (passwords, tokens, request bodies).
- Events are best-effort (e.g. if the DB is unavailable, the system continues running).

## Sessions: last active

The Sessions settings page includes `lastActive`, which updates when PlainNAS successfully decrypts authenticated GraphQL requests for that session.
