# Disk Manager

This document describes the Disk Manager behavior and safety rules.

## Format whole disk (single partition)

PlainNAS supports formatting an entire disk into a **single partition**.

### What it does

When you format a disk:

- All existing data and partitioning information on the disk is erased.
- A new GPT partition table is created.
- A single Linux partition is created.
- The new partition is formatted as **EXT4** (label: `plain-nas`).

Backend implementation uses common Linux utilities:

- `wipefs -a <disk>`
- `sfdisk` (create GPT + one partition)
- `mkfs.ext4 -F -L plain-nas <partition>`

### GraphQL API

- Mutation: `formatDisk(path: String!): Boolean!`
  - `path` must be a `/dev/...` disk path (for example `/dev/sda`, `/dev/nvme0n1`).

### UI behavior

- The Disk Manager shows a **text** button “Format disk” (not an icon) for disks that are allowed to be formatted.
- The format action always requires a confirmation modal.

### Safety rules (important)

Formatting is intentionally strict to avoid destroying the OS disk or mounted disks.

**The backend refuses to format when**:

- The target device is not a `TYPE=disk` (as reported by `lsblk`).
- The disk is the **system/root disk** (any descendant mountpoint is `/`).
- Any descendant partition/device is currently **mounted**.
- Required tools are missing: `wipefs`, `sfdisk`, `mkfs.ext4`.

**The frontend also hides the format button** for the system disk (best-effort detection), but the backend checks are the source of truth.

### Notes

- This feature is Linux-only.
- PlainNAS is typically run with sufficient privileges (often as root) to manage block devices.
- After formatting, the new filesystem is created but **not automatically mounted**; mounting/adding it to storage configuration is a separate step.
