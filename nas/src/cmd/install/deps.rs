//! Dependency installers.
//! On `plainnas install` we install the OS packages needed:
//! ffmpeg (video thumbnails), samba (LAN sharing), and avahi (`.local` discovery).

use super::packages::{InstallPlan, ensure_installed, ensure_samba_vfs_modules_debian_ubuntu};
use super::ui;

pub fn install_all_deps() {
    ui::print_section("Installing dependencies");
    install_ffmpeg();
    install_samba();
    install_avahi();
}

pub fn install_libre_office() {
    ensure_installed(&InstallPlan {
        name: "DOC/DOCX preview (LibreOffice)".into(),
        present_any: vec!["soffice".into(), "libreoffice".into()],
        present_all: vec![],
        apt_pkg: "libreoffice".into(),
        dnf_pkg: "libreoffice".into(),
        yum_pkg: "libreoffice".into(),
        pacman_pkg: "libreoffice-fresh".into(),
        apk_pkg: "libreoffice".into(),
        no_supported_msg: "Couldn't detect a supported package manager to install LibreOffice."
            .into(),
    });
}

pub fn install_avahi() {
    ensure_installed(&InstallPlan {
        name: "Local name discovery (.local)".into(),
        present_any: vec!["avahi-daemon".into(), "avahi-browse".into()],
        present_all: vec![],
        apt_pkg: "avahi-daemon avahi-utils libnss-mdns".into(),
        dnf_pkg: "avahi avahi-tools nss-mdns".into(),
        yum_pkg: "avahi avahi-tools nss-mdns".into(),
        pacman_pkg: "avahi nss-mdns".into(),
        apk_pkg: "avahi avahi-tools".into(),
        no_supported_msg: "Couldn't detect a supported package manager to install Avahi.".into(),
    });
    if super::packages::has_cmd("systemctl") {
        let _ = ui::run_progress(
            "Enable & start avahi-daemon",
            "systemctl enable --now avahi-daemon",
        );
    }
}

pub fn install_samba() {
    ensure_installed(&InstallPlan {
        name: "LAN sharing (Samba)".into(),
        present_any: vec![],
        present_all: vec!["smbd".into(), "smbpasswd".into()],
        apt_pkg: "samba".into(),
        dnf_pkg: "samba".into(),
        yum_pkg: "samba".into(),
        pacman_pkg: "samba".into(),
        apk_pkg: "samba".into(),
        no_supported_msg: "Couldn't detect a supported package manager to install Samba.".into(),
    });
    ensure_samba_vfs_modules_debian_ubuntu();
}

pub fn install_ffmpeg() {
    ensure_installed(&InstallPlan {
        name: "Video thumbnails (ffmpeg)".into(),
        present_any: vec![],
        present_all: vec!["ffmpeg".into(), "ffprobe".into()],
        apt_pkg: "ffmpeg".into(),
        dnf_pkg: "ffmpeg".into(),
        yum_pkg: "ffmpeg".into(),
        pacman_pkg: "ffmpeg".into(),
        apk_pkg: "ffmpeg".into(),
        no_supported_msg: "Couldn't detect a supported package manager to install ffmpeg.".into(),
    });
}
