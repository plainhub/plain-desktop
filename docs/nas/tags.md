# Tags

This document describes PlainNAS tag behavior, requirements, and the current implementation.

## Concepts

- **Tag**: a named label, stored as a record with fields like `id`, `name`, `type`, and `count`.
- **Tag relation**: a link between a tag and an item key.
  - For media, the key is the media UUID.
  - Relations are stored as **key presence** in the KV store (value is a constant `1`), not as JSON payloads.

## Data model (KV keys)

- Tag record:
  - Key: `tag:<tagID>`
  - Value: JSON of `db.Tag`

- Tag relation primary record:
  - Key: `tag_relation:<tagID>:<key>`
  - Value: `1`

- Tag relation secondary index (by item key):
  - Key: `tag_relation_key:<key>:<tagID>`
  - Value: `1`

Notes:
- The secondary index enables a fast lookup for “what tags are on this item key?”
- `EnsureTagRelationKeyIndex()` can backfill the `tag_relation_key:` index from existing `tag_relation:` keys.

## Requirements

### 1) `tag.count` must be correct

`tag.count` must reflect the number of existing relations for that tag:

- `tag.count == number_of(tag_relation:<tagID>:...)`

It must be updated when:
- Adding tag relations.
- Removing tag relations.
- Running a reindex/rebuild operation.

### 2) Idempotent behavior

Repeated “add the same relation” operations must not inflate `tag.count`. The relation key is unique (`tag_relation:<tagID>:<key>`), so a repeated save overwrites the same record and the derived count remains stable.

### 3) Deleting / trashing items removes relations

If a file/media item is deleted or moved to trash, its tag relations must be removed (so tags never point at trash/missing items).

## Implementation

### Where tag relations are mutated

- GraphQL mutations call into DB helpers:
  - `UpdateTagRelations` adds/removes relations for a single item.
  - Bulk operations like `addToTags` / `removeFromTags` add/remove relations for a query result.

The actual persistence happens in the DB layer:
- `SaveTagRelation` / `SaveTagRelations`
- `DeleteTagRelationsByTagID`
- `DeleteTagRelationsByKeys...`

### How `tag.count` is maintained

The authoritative source of truth is the **relation records**, not the stored `Tag.Count` value.

- On every relation **add** (save):
  - Persist `tag_relation:` and `tag_relation_key:`
  - Recompute count for that tag and update the tag record.

- On relation **remove** (delete):
  - Delete `tag_relation:` and `tag_relation_key:`
  - Recompute count for every affected tag and update their tag records.

Helpers:
- `RecomputeTagCount(tagID)`:
  - Iterates `tag_relation:<tagID>:` prefix and counts entries
  - Writes the result back to the tag record

- `RebuildAllTagCounts()`:
  - Scans all `tag_relation:` keys once to build `tagID -> count`
  - Iterates all tags and updates each `Tag.Count` accordingly

### Reindex behavior

Media reindexing ("rebuild media index") resets media data and indexes. Tag relations are independent from media records, so a reset can leave behind **stale relations** (UUIDs that no longer exist after the rebuild).

After the reindex scan completes, PlainNAS performs a cleanup pass:

- Iterates `tag_relation_key:*` and deletes relations for UUIDs that are missing from the rebuilt media store
- Runs `RebuildAllTagCounts()` to repair all stored `tag.count` values

Implementation notes:
- Media trash: when a media item is moved to trash, its UUID relations are removed.
- Media delete: when a media item is deleted from the media store, its UUID relations are removed.
- Reindex cleanup is only triggered after `ResetAllMediaData()` (full rebuild).

## Testing

The following regressions are covered:

- Add relation => `tag.count` increments.
- Add the same relation again => `tag.count` remains correct.
- Remove relation => `tag.count` decrements.
- Rebuild counts => fixes intentionally incorrect stored `tag.count`.

## Troubleshooting

If tag counts look wrong:

1. Ensure relations exist as expected:
   - `tag_relation:<tagID>:...` keys should match the desired tag usage.
2. Run a count rebuild (development/admin flow):
   - Trigger a media index rebuild (which also rebuilds tag counts), or call the backend helper if you have an internal admin route.
3. If your DB is old and missing `tag_relation_key:` keys:
   - Run `EnsureTagRelationKeyIndex()` once to backfill the by-key index.

## Notes for contributors

- Avoid maintaining `tag.count` via incremental +1/-1 updates unless you can guarantee transactional correctness across all write paths.
- Prefer deriving `tag.count` from relation keys to keep invariants simple and resilient to unexpected or legacy data states.
