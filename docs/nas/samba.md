# LAN Share (SMB / Samba)

PlainNAS can share folders over your local network using SMB (Samba 3).

> **Current status (Rust port):** Samba integration is **stub-only**. The settings UI can be filled in and saved (values are persisted to the KV store and round-tripped back), but the backend does **not** write `/etc/samba/smb.conf` and does **not** restart the `smbd` service yet. `setSambaUserPassword` also returns `true` without actually changing the system password. See `src/samba.rs`.

## Configure

In the Web UI:

- Settings → LAN Share
- Access modes:
  - Anyone can modify (guest)
  - Anyone read-only (guest)
  - Password required (read-write)
  - Password required (read-only)

If you select **Password required**, you must set a password at least once. After that, you can leave it blank to keep the current password.

## How to access

- Windows: `\\<NAS-IP>\<share-name>`
- macOS: Finder → Go → Connect to Server → `smb://<NAS-IP>/<share-name>`
- Linux: `smb://<NAS-IP>/<share-name>` (file manager) or `mount -t cifs`

## Notes

- Ensure Samba is installed: `sudo apt-get install samba` (or use `sudo plain-nas install` which installs system dependencies).
- Samba VFS modules (for macOS Finder compatibility via the `fruit` module): `sudo apt-get install samba-vfs-modules`.
- Verify module exists:
  - `smbd -b | grep MODULESDIR`
  - `ls -la "$(smbd -b | awk -F': ' '/^MODULESDIR:/{print $2}')/vfs" | grep fruit`
