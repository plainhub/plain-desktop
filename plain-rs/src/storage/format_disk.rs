//! Disk format. 1:1 port of `internal/graph/storage_format_disk_api.go`.
//!
//! Wipes a whole disk (not a partition) and creates a single Linux ext4
//! partition on a fresh GPT label. The Go side uses `lsblk`, `wipefs`,
//! `sfdisk`, `partprobe` / `udevadm`, and `mkfs.ext4`; we keep the exact same
//! toolchain. None of these commands are bundled — they must exist on the
//! host (the install script installs them on Debian/Ubuntu).
//!
//! Safety:
//! - Refuses paths that don't start with `/dev/`
//! - Refuses to format a disk (or any of its partitions) that has `/`
//!   mounted — protects the system/root disk.
//! - Refuses to format anything whose `lsblk TYPE` isn't `disk`.
//!
//! While formatting, the automount reconciler is inhibited so it can't
//! remount what we just unmounted; once formatting completes, one
//! reconciliation pass mounts the fresh filesystem into its `/mnt/usbX`
//! slot (Go does the same dance around `EnsureMountedUSBVolumes`).
//!
//! This is a destructive operation; it requires root. The `cmd run` flow
//! already enforces root, so no extra check here.

use crate::storage::blockdev::{LsblkDevice, run_lsblk};
use anyhow::{Result, anyhow};
use std::io::Write;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

/// Run a command, capturing combined output. Returns `Err` with a one-line
/// summary on non-zero exit.
fn run_cmd(prog: &str, args: &[&str]) -> Result<String> {
    let out = Command::new(prog)
        .args(args)
        .output()
        .map_err(|e| anyhow!("{prog} spawn: {e}"))?;
    if !out.status.success() {
        let combined = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(anyhow!(
            "{prog} failed: {}",
            one_line(&format!("{combined}{stderr}"))
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn one_line(s: &str) -> String {
    s.replace(['\n', '\r'], " ").trim().to_string()
}

/// Compute the first-partition device path: `sda`→`sda1`, `nvme0n1`→`nvme0n1p1`.
/// Mirrors Go `firstPartitionPath`.
pub fn first_partition_path(disk_path: &str) -> String {
    let base = disk_path.trim();
    if base.is_empty() {
        return String::new();
    }
    let last = base.chars().last().unwrap();
    if last.is_ascii_digit() {
        format!("{base}p1")
    } else {
        format!("{base}1")
    }
}

/// Recursively search an `lsblk` device tree for a disk matching `path`.
fn find_lsblk_disk<'a>(devs: &'a [LsblkDevice], path: &str) -> Option<&'a LsblkDevice> {
    fn walk<'a>(d: &'a LsblkDevice, path: &str, out: &mut Option<&'a LsblkDevice>) {
        if out.is_some() {
            return;
        }
        let p = d.path.trim();
        let n = d.name.trim();
        let full = std::path::Path::new("/dev").join(n);
        if p == path || (p.is_empty() && !n.is_empty() && full.to_string_lossy() == path) {
            *out = Some(d);
            return;
        }
        for c in &d.children {
            walk(c, path, out);
        }
    }
    let mut found = None;
    for d in devs {
        walk(d, path, &mut found);
        if found.is_some() {
            break;
        }
    }
    found
}

fn has_mountpoint(d: &LsblkDevice, m: &str) -> bool {
    if d.mountpoint.as_deref().map(str::trim) == Some(m) {
        return true;
    }
    d.children.iter().any(|c| has_mountpoint(c, m))
}

fn list_mounted(d: &LsblkDevice) -> Vec<String> {
    fn walk(d: &LsblkDevice, out: &mut Vec<String>) {
        let mp = d.mountpoint.as_deref().unwrap_or("").trim();
        if !mp.is_empty() && !out.iter().any(|s| s == mp) {
            out.push(mp.to_string());
        }
        for c in &d.children {
            walk(c, out);
        }
    }
    let mut v = Vec::new();
    walk(d, &mut v);
    v
}

fn lsblk() -> Result<Vec<LsblkDevice>> {
    run_lsblk(&["NAME", "PATH", "TYPE", "MOUNTPOINT"])
}

fn which(prog: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {prog}"))
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn require_tool(prog: &str) -> Result<()> {
    if which(prog) {
        Ok(())
    } else {
        Err(anyhow!("required tool not found in PATH: {prog}"))
    }
}

/// Wait up to `timeout` for `path` to appear.
fn wait_for_path(path: &str, timeout: Duration) -> Result<()> {
    if path.trim().is_empty() {
        return Err(anyhow!("path is empty"));
    }
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if Path::new(path).exists() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(80));
    }
    Err(anyhow!("timeout waiting for {path}"))
}

/// Unmount `mountpoints` deepest-first, calling `on_unmount` after each
/// successful unmount. Returns Err on first failure. Go
/// `unmountMountpoints`.
fn unmount_mountpoints<F: FnMut(&str)>(mountpoints: &[String], mut on_unmount: F) -> Result<()> {
    let mut mps: Vec<&str> = mountpoints
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    mps.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| b.cmp(a)));
    mps.dedup();
    for mp in mps {
        run_cmd("umount", &[mp])?;
        on_unmount(mp);
    }
    Ok(())
}

/// The main entry point. Wipes the disk at `disk_path` and writes a fresh
/// GPT + ext4 partition. `on_unmount` is called once per successfully
/// unmounted mount point (used by the GraphQL resolver to log events).
/// After formatting, one automount reconciliation mounts the new
/// filesystem into a stable `/mnt/usbX` slot.
pub fn format_disk_single_partition<F: FnMut(&str)>(
    prefs: &std::sync::Arc<crate::prefs::Prefs>,
    disk_path: &str,
    mut on_unmount: F,
) -> Result<()> {
    let p = disk_path.trim();
    if p.is_empty() {
        return Err(anyhow!("disk path is required"));
    }
    if !p.starts_with("/dev/") {
        return Err(anyhow!("invalid disk path: {p:?}"));
    }

    // Prevent the automount reconciler from racing with this formatting
    // operation (it can remount a filesystem right after we unmount it).
    let release_auto_mount = crate::storage::automount::inhibit();

    // Tool availability
    for t in ["wipefs", "sfdisk", "mkfs.ext4", "umount"] {
        require_tool(t)?;
    }
    let has_partprobe = which("partprobe");
    let has_udevadm = which("udevadm");

    // Initial lsblk + safety checks
    let parsed = lsblk()?;
    let disk = find_lsblk_disk(&parsed, p).ok_or_else(|| anyhow!("disk not found: {p}"))?;
    if disk.kind.trim() != "disk" {
        return Err(anyhow!("device is not a disk: {p}"));
    }
    if has_mountpoint(disk, "/") {
        return Err(anyhow!("refusing to format system disk: {p}"));
    }

    // Unmount anything currently mounted on the disk
    let mut mps = list_mounted(disk);
    if !mps.is_empty() {
        unmount_mountpoints(&mps, |mp| on_unmount(mp))?;

        // Re-check from fresh lsblk
        let parsed2 = lsblk()?;
        let disk2 = find_lsblk_disk(&parsed2, p).ok_or_else(|| anyhow!("disk not found: {p}"))?;
        if has_mountpoint(disk2, "/") {
            return Err(anyhow!("refusing to format system disk: {p}"));
        }
        mps = list_mounted(disk2);
        if !mps.is_empty() {
            return Err(anyhow!(
                "failed to unmount disk {p} (still mounted: {})",
                mps.join(", ")
            ));
        }
    }

    // Wipe signatures
    run_cmd("wipefs", &["-a", p])?;

    // New GPT + single Linux partition
    let script = "label: gpt\nsize=+, type=linux\n";
    let sfdisk = Command::new("sfdisk")
        .args([
            "--wipe",
            "always",
            "--wipe-partitions",
            "always",
            "--force",
            p,
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| anyhow!("sfdisk spawn: {e}"))?;
    {
        let mut stdin = sfdisk
            .stdin
            .as_ref()
            .ok_or_else(|| anyhow!("sfdisk stdin"))?;
        stdin.write_all(script.as_bytes())?;
    }
    let out = sfdisk.wait_with_output()?;
    if !out.status.success() {
        return Err(anyhow!(
            "sfdisk failed: {}",
            one_line(&format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ))
        ));
    }

    if has_partprobe {
        let _ = Command::new("partprobe").arg(p).output();
    }
    if has_udevadm {
        let _ = Command::new("udevadm").arg("settle").output();
    }

    let part_path = first_partition_path(p);
    wait_for_path(&part_path, Duration::from_secs(3))?;

    run_cmd("mkfs.ext4", &["-F", "-L", "plainnas", &part_path])?;

    // Formatting complete: lift inhibition and reconcile once so the new
    // filesystem gets mounted into a stable /mnt/usbX slot (bounded like
    // Go's 20s context).
    drop(release_auto_mount);
    crate::storage::automount::ensure_mounted_with_timeout(prefs, Duration::from_secs(20));

    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/storage/format_disk.rs"]
mod tests;
