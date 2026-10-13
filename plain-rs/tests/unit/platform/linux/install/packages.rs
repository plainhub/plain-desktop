use super::*;

fn empty_plan() -> InstallPlan {
    InstallPlan {
        name: "test-pkg".into(),
        present_any: vec![],
        present_all: vec![],
        apt_pkg: "test-pkg".into(),
        dnf_pkg: "test-pkg".into(),
        yum_pkg: "test-pkg".into(),
        pacman_pkg: "test-pkg".into(),
        apk_pkg: "test-pkg".into(),
        no_supported_msg: "no support".into(),
    }
}

#[test]
fn present_any_skips_install() {
    let mut p = empty_plan();
    p.present_any = vec!["bash".into()];
    assert!(has_any_in_path(&p.present_any));
    assert!(!has_all_in_path(&[]));
}

#[test]
fn present_all_requires_every_binary() {
    assert!(has_all_in_path(&["bash".into()]));
    assert!(!has_all_in_path(&[
        "definitely-not-on-path".into(),
        "bash".into()
    ]));
}

#[test]
fn detect_pkg_manager_returns_known_or_empty() {
    let pm = detect_pkg_manager();
    assert!(
        ["apt-get", "apt", "dnf", "yum", "pacman", "apk", ""].contains(&pm),
        "unexpected pm: {pm}",
    );
}
