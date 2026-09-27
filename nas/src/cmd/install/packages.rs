//! Package manager detection + `ensure_installed` helper. Mirrors
//! `cmd/install/packages.go` in the Go side. Supports apt-get, apt, dnf,
//! yum, pacman and apk (in that order of preference).

use std::process::Command;

use super::ui;

#[derive(Clone, Debug)]
pub struct InstallPlan {
    pub name: String,
    pub present_any: Vec<String>,
    pub present_all: Vec<String>,
    pub apt_pkg: String,
    pub dnf_pkg: String,
    pub yum_pkg: String,
    pub pacman_pkg: String,
    pub apk_pkg: String,
    pub no_supported_msg: String,
}

pub fn has_cmd(name: &str) -> bool {
    which(name).is_some()
}

pub fn has_any_in_path(names: &[String]) -> bool {
    names.iter().any(|n| has_cmd(n))
}
pub fn has_all_in_path(names: &[String]) -> bool {
    !names.is_empty() && names.iter().all(|n| has_cmd(n))
}

pub fn detect_pkg_manager() -> &'static str {
    for pm in ["apt-get", "apt", "dnf", "yum", "pacman", "apk"] {
        if has_cmd(pm) {
            return pm;
        }
    }
    ""
}

fn which(name: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    for p in std::env::split_paths(&path) {
        let candidate = p.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Run `cmd` and return its trimmed stdout. Used for probing package
/// state (e.g. `dpkg-query -W`).
pub fn exec_capture(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn ensure_installed(plan: &InstallPlan) {
    if has_any_in_path(&plan.present_any) || has_all_in_path(&plan.present_all) {
        if !plan.name.is_empty() {
            ui::print_ok(&plan.name, "Already installed");
        }
        return;
    }
    let label = if plan.name.is_empty() {
        "Install packages"
    } else {
        &plan.name
    };

    match detect_pkg_manager() {
        "apt-get" => {
            if plan.apt_pkg.is_empty() {
                ui::print_note(label, &plan.no_supported_msg);
                return;
            }
            ui::print_note(label, "Installing...");
            let _ = ui::run_progress(
                "Update package index",
                "DEBIAN_FRONTEND=noninteractive apt-get update",
            );
            if ui::run_progress(
                label,
                &format!(
                    "DEBIAN_FRONTEND=noninteractive apt-get install -y {}",
                    plan.apt_pkg
                ),
            )
            .is_err()
            {
                ui::print_fail(label, "Install failed. You can try installing it manually.");
                return;
            }
            ui::print_ok(label, "Installed");
        }
        "apt" => {
            if plan.apt_pkg.is_empty() {
                ui::print_note(label, &plan.no_supported_msg);
                return;
            }
            ui::print_note(label, "Installing...");
            let _ = ui::run_progress(
                "Update package index",
                "DEBIAN_FRONTEND=noninteractive apt update",
            );
            if ui::run_progress(
                label,
                &format!(
                    "DEBIAN_FRONTEND=noninteractive apt install -y {}",
                    plan.apt_pkg
                ),
            )
            .is_err()
            {
                ui::print_fail(label, "Install failed. You can try installing it manually.");
                return;
            }
            ui::print_ok(label, "Installed");
        }
        "dnf" => {
            if plan.dnf_pkg.is_empty() {
                ui::print_note(label, &plan.no_supported_msg);
                return;
            }
            ui::print_note(label, "Installing...");
            if ui::run_progress(label, &format!("dnf install -y {}", plan.dnf_pkg)).is_err() {
                ui::print_fail(label, "Install failed. You can try installing it manually.");
                return;
            }
            ui::print_ok(label, "Installed");
        }
        "yum" => {
            if plan.yum_pkg.is_empty() {
                ui::print_note(label, &plan.no_supported_msg);
                return;
            }
            ui::print_note(label, "Installing...");
            if ui::run_progress(label, &format!("yum install -y {}", plan.yum_pkg)).is_err() {
                ui::print_fail(label, "Install failed. You can try installing it manually.");
                return;
            }
            ui::print_ok(label, "Installed");
        }
        "pacman" => {
            if plan.pacman_pkg.is_empty() {
                ui::print_note(label, &plan.no_supported_msg);
                return;
            }
            ui::print_note(label, "Installing...");
            if ui::run_progress(
                label,
                &format!("pacman -Sy --noconfirm {}", plan.pacman_pkg),
            )
            .is_err()
            {
                ui::print_fail(label, "Install failed. You can try installing it manually.");
                return;
            }
            ui::print_ok(label, "Installed");
        }
        "apk" => {
            if plan.apk_pkg.is_empty() {
                ui::print_note(label, &plan.no_supported_msg);
                return;
            }
            ui::print_note(label, "Installing...");
            if ui::run_progress(label, &format!("apk add --no-cache {}", plan.apk_pkg)).is_err() {
                ui::print_fail(label, "Install failed. You can try installing it manually.");
                return;
            }
            ui::print_ok(label, "Installed");
        }
        _ => ui::print_note(label, &plan.no_supported_msg),
    }
}

/// On Debian/Ubuntu the `samba-vfs-modules` package provides macOS
/// Finder compatibility (Apple SMB extensions). Mirrors Go's
/// `ensureSambaVfsModulesDebianUbuntu`.
pub fn ensure_samba_vfs_modules_debian_ubuntu() {
    if !matches!(detect_pkg_manager(), "apt-get" | "apt") {
        return;
    }
    if !has_cmd("dpkg-query") {
        return;
    }
    let out = match exec_capture("dpkg-query", &["-W", "-f=${Status}", "samba-vfs-modules"]) {
        Some(s) => s,
        None => {
            let _ = ui::run_progress(
                "Install macOS Samba compatibility",
                "DEBIAN_FRONTEND=noninteractive apt-get install -y samba-vfs-modules",
            );
            return;
        }
    };
    if out.contains("install ok installed") {
        return;
    }
    let _ = ui::run_progress(
        "Install macOS Samba compatibility",
        "DEBIAN_FRONTEND=noninteractive apt-get install -y samba-vfs-modules",
    );
}

#[cfg(test)]
#[path = "../../../tests/unit/cmd/install/packages.rs"]
mod tests;
