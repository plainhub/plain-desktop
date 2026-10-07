# Trash

PlainNAS has two trash flows:

PlainNAS has a single unified trash for both Files and Media.

## Unified Trash

- Location: per-disk `${MOUNT}/.plain-trash` (one trash directory per filesystem / physical disk)
- Used by: Files view and Media (Images/Videos/Audios)
- Implementation (core rules):
	- Delete is always a single `rename(2)` (O(1))
	- Never traverses directories; never touches children
	- Never performs cross-filesystem copy
	- Trash contents are bucketed by date only for performance/GC (business logic must not depend on it)
	- Restore/GC logic uses KV metadata as the single source of truth
- Directory layout (per disk):

```
${MOUNT}/.plain-trash/
	data/YYYY/MM/f_<id>
	data/YYYY/MM/d_<id>
	.lock
```

- Metadata storage:
	- Stored in the existing default KV DB (`db::get_default()`) under the `trash:*` key namespace
	- No sidecar `.metadata` files are required for correctness

- Code:
	- Files trash implementation: `src/trash.rs`
	- Media actions call into the same implementation (no separate media trash directory)

Notes:
- `${DATA_DIR}` defaults to `/var/lib/plain-nas` (see `src/consts.rs`).
- `.plain-trash` is a hidden directory and is excluded from indexing/scans.
- Mountpoint detection uses `/proc/self/mountinfo` and falls back to resolving symlinked path components when needed (e.g. if a mount is accessed via a symlink like `/DATA`).
