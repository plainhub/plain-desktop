//! Samba shares — full port of Go `internal/samba/samba.go` +
//! `internal/graph/samba_api.go`.
//!
//! Applying settings renders `/etc/samba/smb.conf` (with macOS-friendly
//! `fruit`/`catia`/`streams_xattr` VFS modules when present), ensures the
//! `nas` unix user, provisions its smbpasswd, and enables + (re)starts the
//! systemd unit (`smbd` / `samba` / `smb`, whichever is loaded). Settings
//! themselves persist in prefs.

use crate::prefs::Prefs;
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::process::Command;

pub const UNIX_USER: &str = "nas";
pub const SMB_CONF_PATH: &str = "/etc/samba/smb.conf";

const KEY_SAMBA_SETTINGS: &str = "samba_settings";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SambaShareAuth {
    Guest,
    Password,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SambaShare {
    pub name: String,
    #[serde(rename = "sharePath")]
    pub share_path: String,
    pub auth: SambaShareAuth,
    #[serde(rename = "readOnly")]
    pub read_only: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SambaSettings {
    pub enabled: bool,
    pub username: String,
    #[serde(rename = "hasPassword")]
    pub has_password: bool,
    pub shares: Vec<SambaShare>,
    #[serde(rename = "serviceName")]
    pub service_name: String,
    #[serde(rename = "serviceActive")]
    pub service_active: bool,
    #[serde(rename = "serviceEnabled")]
    #[serde(default)]
    pub service_enabled: bool,
}

impl Default for SambaSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            username: UNIX_USER.to_string(),
            has_password: false,
            shares: Vec::new(),
            service_name: "smbd".to_string(),
            service_active: false,
            service_enabled: false,
        }
    }
}

pub fn get_samba_settings(prefs: &Prefs) -> SambaSettings {
    prefs
        .get::<SambaSettings>(KEY_SAMBA_SETTINGS)
        .ok()
        .flatten()
        .unwrap_or_default()
}

pub fn set_samba_settings(prefs: &Prefs, settings: &SambaSettings) -> Result<()> {
    prefs
        .set(KEY_SAMBA_SETTINGS, settings)
        .map_err(|e| anyhow!("{e}"))?;
    Ok(())
}

// ----- service status -----

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ServiceStatus {
    pub active: bool,
    pub enabled: bool,
    pub name: String,
}

/// Live unit status. Go `GetServiceStatus` (we already run as root, so no
/// `sudo`). No systemd / not Linux → zero status.
pub fn get_service_status() -> ServiceStatus {
    let name = detect_systemd_service_name();
    if name.is_empty() {
        return ServiceStatus::default();
    }
    let out = systemctl_show(&name, &["UnitFileState", "ActiveState"]);
    ServiceStatus {
        active: out.contains("=active"),
        enabled: out.contains("=enabled"),
        name,
    }
}

/// First loaded unit among `smbd` / `samba` / `smb`. Go
/// `detectSystemdServiceName`.
pub fn detect_systemd_service_name() -> String {
    for name in ["smbd", "samba", "smb"] {
        if is_systemd_unit_loaded(name) {
            return name.to_string();
        }
    }
    String::new()
}

fn is_systemd_unit_loaded(service_name: &str) -> bool {
    systemctl_show(service_name, &["LoadState"]).contains("LoadState=loaded")
}

fn systemctl_show(service: &str, props: &[&str]) -> String {
    let mut args: Vec<String> = vec!["show".to_string(), service.to_string()];
    args.push(format!("--property={}", props.join(",")));
    match Command::new("systemctl").args(&args).output() {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => String::new(),
    }
}

// ----- apply -----

/// Validate the desired settings before touching the system: enabled
/// requires at least one share; every share path must be an existing (or
/// creatable) directory. Go `Apply`'s front half.
pub fn validate_shares(enabled: bool, shares: &[SambaShare]) -> Result<()> {
    if !enabled {
        return Ok(());
    }
    if shares.is_empty() {
        return Err(anyhow!("no shares configured"));
    }
    for sh in shares {
        if sh.share_path.trim().is_empty() {
            return Err(anyhow!("share path is empty"));
        }
        std::fs::create_dir_all(&sh.share_path)?;
        let st = std::fs::metadata(&sh.share_path)?;
        if !st.is_dir() {
            return Err(anyhow!("share path is not a directory"));
        }
    }
    Ok(())
}

/// Apply desired settings to the system: user, smb.conf, password, service.
/// The caller stores desired settings first; the passed value is the source
/// of truth (Go `Apply`). When `password` is non-empty and samba is being
/// enabled, the password is provisioned and `has_password` persisted — the
/// updated settings are returned.
pub fn apply(prefs: &Prefs, settings: &SambaSettings, password: &str) -> Result<SambaSettings> {
    let mut desired = settings.clone();

    validate_shares(desired.enabled, &desired.shares)?;

    ensure_unix_user()?;

    write_smb_conf(&desired)?;

    if desired.enabled && !password.trim().is_empty() {
        ensure_samba_password(password)?;
        desired.has_password = true;
        set_samba_settings(prefs, &desired)?;
    }

    let service = detect_systemd_service_name();
    if service.is_empty() {
        return Err(anyhow!("systemd service for samba not found"));
    }

    if !desired.enabled {
        // Stop + disable; failures ignored like the Go side.
        let _ = systemctl_run(&service, "stop");
        let _ = systemctl_run(&service, "disable");
        return Ok(desired);
    }

    systemctl_run(&service, "enable")?;
    systemctl_run(&service, "restart")?;
    Ok(desired)
}

fn systemctl_run(service: &str, action: &str) -> Result<String> {
    let out = Command::new("systemctl")
        .args([action, service])
        .output()
        .map_err(|e| anyhow!("systemctl {action} spawn: {e}"))?;
    if !out.status.success() {
        let s = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        return Err(anyhow!("systemctl {action} {service} failed: {}", s.trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// Create the `nas` system user if missing. Go `ensureUnixUser` (we run as
/// root, no sudo).
pub fn ensure_unix_user() -> Result<()> {
    if Command::new("id")
        .arg("-u")
        .arg(UNIX_USER)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        return Ok(());
    }
    if which("useradd") {
        // -M: no home, -r: system user (best-effort), -s: no login
        let out = Command::new("useradd")
            .args(["-M", "-r", "-s", "/usr/sbin/nologin", UNIX_USER])
            .output()?;
        return check_run(out, "useradd");
    }
    if which("adduser") {
        // Alpine: -D creates user with no password, -H no home
        let out = Command::new("adduser")
            .args(["-D", "-H", "-s", "/sbin/nologin", UNIX_USER])
            .output()?;
        return check_run(out, "adduser");
    }
    Err(anyhow!(
        "cannot create unix user {UNIX_USER:?}: useradd/adduser not found"
    ))
}

fn check_run(out: std::process::Output, prog: &str) -> Result<()> {
    if out.status.success() {
        Ok(())
    } else {
        let s = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        Err(anyhow!("{prog} failed: {}", s.trim()))
    }
}

/// Provision the samba password for the `nas` user. Go `ensureSambaPassword`
/// (`smbpasswd -a -s` reads the password twice from stdin).
pub fn ensure_samba_password(password: &str) -> Result<()> {
    let pw = password.trim();
    if pw.is_empty() {
        return Ok(());
    }
    if !which("smbpasswd") {
        return Err(anyhow!("smbpasswd not found"));
    }
    let mut child = Command::new("smbpasswd")
        .args(["-a", "-s", UNIX_USER])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| anyhow!("smbpasswd spawn: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(format!("{pw}\n{pw}\n").as_bytes())?;
    }
    let out = child.wait_with_output()?;
    if !out.status.success() {
        let s = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        return Err(anyhow!("smbpasswd failed: {}", s.trim()));
    }
    Ok(())
}

/// Set the samba user password standalone (`setSambaUserPassword` mutation).
pub fn set_user_password(password: &str) -> Result<()> {
    ensure_unix_user()?;
    ensure_samba_password(password)
}

// ----- smb.conf rendering -----

/// VFS module availability injected into `render_smb_conf` (deterministic
/// tests); the real probe asks `smbd -b` / the filesystem.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VfsCaps {
    pub fruit: bool,
    pub catia: bool,
    pub streams_xattr: bool,
}

/// Pure renderer: the full smb.conf for the desired settings. Go
/// `writeSmbConf`'s string building.
pub fn render_smb_conf<F: Fn(&str) -> bool>(
    s: &SambaSettings,
    caps: VfsCaps,
    xattr_ok: F,
) -> String {
    let mut b = String::new();
    b.push_str("# Managed by PlainNAS.\n");
    b.push_str("# Manual edits may be overwritten from the Web Settings page.\n\n");
    b.push_str("[global]\n");
    b.push_str("  workgroup = WORKGROUP\n");
    b.push_str("  server role = standalone server\n");
    b.push_str("  map to guest = Bad User\n");
    b.push_str("  load printers = no\n");
    b.push_str("  disable spoolss = yes\n");
    b.push_str("  log file = /var/log/samba/log.%m\n");
    b.push_str("  max log size = 1000\n");
    b.push_str("  server min protocol = SMB2\n");
    if caps.fruit {
        // Apple SMB2 extensions (AAPL) — improves Finder interoperability.
        b.push_str("  fruit:aapl = yes\n");
    }
    b.push('\n');

    if s.enabled {
        let mut used: std::collections::HashSet<String> = std::collections::HashSet::new();
        for sh in &s.shares {
            let mut name = sanitize_share_name(&sh.name);
            if name.is_empty() {
                name = "share".to_string();
            }
            // Ensure uniqueness.
            let base = name.clone();
            let mut n = 2;
            while used.contains(&name.to_lowercase()) {
                name = format!("{base}-{n}");
                n += 1;
            }
            used.insert(name.to_lowercase());

            b.push_str(&format!("[{name}]\n"));
            b.push_str(&format!("  path = {}\n", sh.share_path));
            b.push_str("  browseable = yes\n");

            // macOS compatibility: fruit + (optional) streams_xattr when
            // possible. For filesystems without xattr support (common for
            // USB/external drives), fall back to AppleDouble sidecar files.
            if caps.fruit {
                let xattr_ok = xattr_ok(&sh.share_path);
                let mut vfs: Vec<&str> = Vec::with_capacity(3);
                if caps.catia {
                    vfs.push("catia");
                }
                vfs.push("fruit");
                if xattr_ok && caps.streams_xattr {
                    vfs.push("streams_xattr");
                    b.push_str("  ea support = yes\n");
                    b.push_str("  store dos attributes = yes\n");
                    b.push_str("  fruit:metadata = stream\n");
                    b.push_str("  fruit:resource = stream\n");
                } else {
                    b.push_str("  fruit:metadata = netatalk\n");
                    b.push_str("  fruit:resource = file\n");
                }

                // Common Finder-friendly settings.
                b.push_str("  fruit:posix_rename = yes\n");
                b.push_str("  fruit:zero_file_id = yes\n");
                b.push_str("  fruit:delete_empty_adfiles = yes\n");
                b.push_str(&format!("  vfs objects = {}\n", vfs.join(" ")));
            }

            b.push_str(&format!("  force user = {UNIX_USER}\n"));
            b.push_str("  create mask = 0664\n");
            b.push_str("  directory mask = 0775\n");

            match sh.auth {
                SambaShareAuth::Guest => {
                    b.push_str("  guest ok = yes\n");
                    b.push_str("  guest only = yes\n");
                }
                SambaShareAuth::Password => {
                    b.push_str("  guest ok = no\n");
                    b.push_str(&format!("  valid users = {UNIX_USER}\n"));
                }
            }

            b.push_str(if sh.read_only {
                "  read only = yes\n"
            } else {
                "  read only = no\n"
            });
            b.push('\n');
        }
    }

    b
}

/// Go `sanitizeShareName`: `[a-zA-Z0-9._-]` (spaces → `_`), trimmed of
/// leading/trailing `-_. `, max 32 chars.
pub fn sanitize_share_name(name: &str) -> String {
    let s = name.trim().trim_matches(|c| c == '[' || c == ']');
    if s.is_empty() {
        return String::new();
    }
    let mut out: String = s
        .chars()
        .map(|r| match r {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' => r,
            _ => '_',
        })
        .collect();
    out = out
        .trim_matches(|c: char| c == '-' || c == '_' || c == '.' || c == ' ')
        .to_string();
    if out.chars().count() > 32 {
        out = out.chars().take(32).collect();
    }
    out
}

/// Write the rendered conf to `SMB_CONF_PATH` and validate it best-effort
/// with `testparm -s`. Go `writeSmbConf`.
pub fn write_smb_conf(s: &SambaSettings) -> Result<()> {
    if let Some(parent) = std::path::Path::new(SMB_CONF_PATH).parent() {
        std::fs::create_dir_all(parent)?;
    }
    let caps = VfsCaps {
        fruit: vfs_module_available("fruit"),
        catia: vfs_module_available("catia"),
        streams_xattr: vfs_module_available("streams_xattr"),
    };
    let conf = render_smb_conf(s, caps, supports_xattrs);
    std::fs::write(SMB_CONF_PATH, conf)?;
    if which("testparm") {
        let _ = Command::new("testparm")
            .args(["-s", SMB_CONF_PATH])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
    Ok(())
}

/// Best-effort probe: can the filesystem at `dir` hold xattrs? Prefers a
/// temp file inside the directory (more representative); conservative —
/// when we cannot verify, report unsupported. Go `supportsXattrs`.
pub fn supports_xattrs(dir: &str) -> bool {
    #[cfg(not(target_os = "linux"))]
    {
        let _ = dir;
        false
    }
    #[cfg(target_os = "linux")]
    {
        use std::ffi::CString;
        const ATTR: &str = "user.plainnas_xattr_probe";

        fn try_setxattr(path: &str, attr: &str) -> bool {
            let Ok(c_path) = CString::new(path) else {
                return false;
            };
            let Ok(c_attr) = CString::new(attr) else {
                return false;
            };
            let value: &[u8] = b"1";
            // SAFETY: plain FFI call with valid C strings and buffer.
            let rc = unsafe {
                libc::setxattr(
                    c_path.as_ptr(),
                    c_attr.as_ptr(),
                    value.as_ptr() as *const libc::c_void,
                    value.len(),
                    0,
                )
            };
            if rc != 0 {
                return false;
            }
            // SAFETY: same.
            unsafe { libc::removexattr(c_path.as_ptr(), c_attr.as_ptr()) };
            true
        }

        // Probe a temp file inside the directory first.
        let tmp_path =
            std::path::Path::new(dir).join(format!(".plainnas-xattr-{}", std::process::id()));
        match std::fs::File::create(&tmp_path) {
            Ok(_f) => {
                let path = tmp_path.to_string_lossy().to_string();
                let ok = try_setxattr(&path, ATTR);
                let _ = std::fs::remove_file(&tmp_path);
                ok
            }
            Err(_) => try_setxattr(dir, ATTR),
        }
    }
}

/// Is a Samba VFS module installed? Prefers `smbd -b`'s MODULESDIR, then
/// the usual distro library paths. Go `vfsModuleAvailable`.
pub fn vfs_module_available(name: &str) -> bool {
    let candidates = [format!("vfs_{name}.so"), format!("{name}.so")];

    if which("smbd")
        && let Ok(out) = Command::new("smbd").arg("-b").output()
    {
        let text = String::from_utf8_lossy(&out.stdout).to_string();
        for line in text.lines() {
            let line = line.trim();
            if let Some(dir) = line.strip_prefix("MODULESDIR:") {
                let dir = dir.trim();
                if !dir.is_empty() && has_module_in(&candidates, dir) {
                    return true;
                }
            }
        }
    }

    let mut paths: Vec<String> = vec!["/usr/lib/samba".to_string(), "/usr/lib64/samba".to_string()];
    match std::env::consts::ARCH {
        "x86_64" => paths.push("/usr/lib/x86_64-linux-gnu/samba".to_string()),
        "aarch64" => paths.push("/usr/lib/aarch64-linux-gnu/samba".to_string()),
        "arm" => paths.push("/usr/lib/arm-linux-gnueabihf/samba".to_string()),
        _ => {}
    }
    paths.iter().any(|dir| has_module_in(&candidates, dir))
}

fn has_module_in(candidates: &[String], dir: &str) -> bool {
    for so in candidates {
        if std::path::Path::new(dir).join("vfs").join(so).exists() {
            return true;
        }
        if std::path::Path::new(dir).join(so).exists() {
            return true;
        }
    }
    false
}

fn which(prog: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {prog}"))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|st| st.success())
        .unwrap_or(false)
}

#[cfg(test)]
#[path = "../tests/unit/samba.rs"]
mod tests;
