use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    let commit = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let build_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    println!("cargo:rustc-env=PLAIN_NAS_GIT_COMMIT={}", commit);
    println!("cargo:rustc-env=PLAIN_NAS_BUILD_TIME={}", build_time);
    println!("cargo:rerun-if-changed=build.rs");
}
