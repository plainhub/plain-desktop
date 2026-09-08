//! Windows Firewall diagnostics + one-click repair for LAN/mDNS discovery.
//!
//! Windows drops unsolicited inbound UDP (mDNS 224.0.0.251:5353) by default,
//! so this app can announce itself but never *receives* peers unless an
//! inbound Allow rule exists for its exe. The fix rule is program-scoped
//! (no port restriction) because the local server also binds a dynamic TCP
//! port that peers connect to for pairing.
//!
//! Detection runs unelevated via the NetSecurity PowerShell module (Win8+;
//! real floor is Win10 1803+ via WebView2). Repair needs an admin token, so
//! it launches a temp .ps1 through `Start-Process -Verb RunAs` (UAC prompt).

use serde::{Deserialize, Serialize};
use std::io::Write;

/// Firewall rule name used by both the repair command and the NSIS hooks.
pub const FIREWALL_RULE_NAME: &str = "PlainApp";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MdnsFirewallStatus {
    pub supported: bool,
    pub firewall_on: bool,
    pub allow_rule: bool,
    pub block_rule: bool,
    pub exe_path: String,
}

impl MdnsFirewallStatus {
    #[cfg_attr(windows, allow(dead_code))]
    fn unsupported() -> Self {
        Self {
            supported: false,
            firewall_on: false,
            allow_rule: false,
            block_rule: false,
            exe_path: String::new(),
        }
    }
}

/// JSON emitted by the status probe script (all booleans, immune to
/// PowerShell's single-element array collapsing).
/// Only *called* from the Windows branch; kept unguarded for the unit tests.
#[derive(Debug, Deserialize)]
#[cfg_attr(not(windows), allow(dead_code))]
struct FirewallProbe {
    #[serde(rename = "anyProfileOn")]
    any_profile_on: bool,
    allow: bool,
    block: bool,
}

#[cfg_attr(not(windows), allow(dead_code))]
fn ps_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg_attr(not(windows), allow(dead_code))]
fn build_status_script(exe: &str) -> String {
    let exe = ps_quote(exe);
    format!(
        r#"
$ErrorActionPreference = 'SilentlyContinue'
$exe = {exe}
$on = $false
$allow = $false
$block = $false
foreach ($p in Get-NetFirewallProfile) {{
    if ([string]$p.Enabled -eq 'True') {{ $on = $true }}
}}
foreach ($f in Get-NetFirewallApplicationFilter -Program $exe) {{
    foreach ($r in $f | Get-NetFirewallRule) {{
        if ([string]$r.Direction -ne 'Inbound' -or [string]$r.Enabled -ne 'True') {{ continue }}
        $a = [string]$r.Action
        if ($a -eq 'Allow') {{ $allow = $true }}
        if ($a -eq 'Block') {{ $block = $true }}
    }}
}}
[pscustomobject]@{{ anyProfileOn = $on; allow = $allow; block = $block }} | ConvertTo-Json -Compress
"#
    )
}

#[cfg_attr(not(windows), allow(dead_code))]
fn build_fix_script(exe: &str) -> String {
    let exe = ps_quote(exe);
    let name = ps_quote(FIREWALL_RULE_NAME);
    format!(
        r#"
$ErrorActionPreference = 'Continue'
$exe = {exe}
# Drop every inbound rule bound to this exe — the Windows "cancel" popup
# leaves a persistent Block rule that outranks any Allow, so adding alone
# is not enough.
netsh advfirewall firewall delete rule name=all dir=in program="$exe" | Out-Null
netsh advfirewall firewall add rule name={name} dir=in action=allow program="$exe" enable=yes profile=any | Out-Null
"#
    )
}

#[cfg_attr(not(windows), allow(dead_code))]
fn parse_probe(stdout: &str) -> Option<FirewallProbe> {
    serde_json::from_str(stdout.trim()).ok()
}

#[cfg(windows)]
fn write_temp_script(name: &str, content: &str) -> Result<std::path::PathBuf, String> {
    let path = std::env::temp_dir().join(name);
    let mut file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
    file.write_all(content.as_bytes()).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Runs PowerShell hidden and captures stdout (console flash would look
/// broken from a GUI app). `args` must not need extra quoting.
#[cfg(windows)]
fn run_powershell_hidden(args: &[&str]) -> Result<std::process::Output, String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    std::process::Command::new("powershell.exe")
        .arg("-NoProfile")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| e.to_string())
}

/// Read-only firewall state for the current exe. No admin token required.
/// (The `#[tauri::command]` wrappers live in `discover/mod.rs`, like every
/// other command in this module.)
pub fn probe_status() -> Result<MdnsFirewallStatus, String> {
    #[cfg(not(windows))]
    {
        return Ok(MdnsFirewallStatus::unsupported());
    }
    #[cfg(windows)]
    {
        let exe = std::env::current_exe()
            .map_err(|e| format!("current_exe failed: {e}"))?
            .to_string_lossy()
            .into_owned();
        let script = write_temp_script("plainapp-fw-status.ps1", &build_status_script(&exe))?;
        let script_path = script.to_string_lossy().into_owned();
        let out = run_powershell_hidden(&["-WindowStyle", "Hidden", "-File", &script_path])?;
        if !out.status.success() {
            return Err(format!("firewall probe exited {}", out.status.code().unwrap_or(-1)));
        }
        let probe = parse_probe(&String::from_utf8_lossy(&out.stdout))
            .ok_or_else(|| "failed to parse firewall probe output".to_string())?;
        Ok(MdnsFirewallStatus {
            supported: true,
            firewall_on: probe.any_profile_on,
            allow_rule: probe.allow,
            block_rule: probe.block,
            exe_path: exe,
        })
    }
}

/// Spawns the UAC-elevated repair script. Ok once the elevated child is
/// spawned — callers poll `probe_status` until the rule shows up.
pub fn apply_fix() -> Result<(), String> {
    #[cfg(not(windows))]
    {
        return Err("unsupported platform".to_string());
    }
    #[cfg(windows)]
    {
        let exe = std::env::current_exe()
            .map_err(|e| format!("current_exe failed: {e}"))?
            .to_string_lossy()
            .into_owned();
        let script = write_temp_script("plainapp-fw-fix.ps1", &build_fix_script(&exe))?;
        let script_path = script.to_string_lossy().into_owned();
        // Elevate through a hidden launcher: Start-Process -Verb RunAs shows
        // the UAC prompt and exits as soon as the elevated child is spawned
        // (or fails, e.g. the user declines). Non-zero exit = declined/failed.
        let launcher = format!(
            "Start-Process -FilePath powershell.exe -Verb RunAs -WindowStyle Hidden -ArgumentList '-NoProfile','-ExecutionPolicy','Bypass','-WindowStyle','Hidden','-File',{path}",
            path = ps_quote(&script_path),
        );
        let out = run_powershell_hidden(&["-WindowStyle", "Hidden", "-Command", &launcher])?;
        if !out.status.success() {
            return Err("elevation declined or failed".to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ps_quote_escapes_single_quotes() {
        assert_eq!(ps_quote("C:\\Program Files\\PlainApp.exe"), "'C:\\Program Files\\PlainApp.exe'");
        assert_eq!(ps_quote("it's"), "'it''s'");
    }

    #[test]
    fn status_script_embeds_quoted_exe() {
        let script = build_status_script("C:\\Apps\\Plain App\\PlainApp.exe");
        assert!(script.contains("$exe = 'C:\\Apps\\Plain App\\PlainApp.exe'"));
        assert!(script.contains("ConvertTo-Json -Compress"));
    }

    #[test]
    fn fix_script_deletes_then_adds() {
        let script = build_fix_script("C:\\Apps\\PlainApp.exe");
        assert!(script.contains("delete rule name=all dir=in program=\"$exe\""));
        assert!(script.contains("add rule name='PlainApp' dir=in action=allow"));
        assert!(script.contains("profile=any"));
        let delete = script.find("delete rule").expect("delete present");
        let add = script.find("add rule").expect("add present");
        assert!(delete < add, "delete must run before add");
    }

    #[test]
    fn parse_probe_accepts_powershell_json() {
        let probe = parse_probe("{\"anyProfileOn\":true,\"allow\":false,\"block\":true}").unwrap();
        assert!(probe.any_profile_on && probe.block && !probe.allow);
        assert!(parse_probe("garbage").is_none());
        assert!(parse_probe("").is_none());
    }
}
