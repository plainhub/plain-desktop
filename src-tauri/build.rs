fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        use std::{path::PathBuf, process::Command};
        let source = "src/host/macos_ble.m";
        let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
        let object = out.join("macos_ble.o");
        let archive = out.join("libplain_macos_ble.a");
        let architecture = match std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
            Ok("aarch64") => "arm64",
            Ok("x86_64") => "x86_64",
            _ => panic!("Unsupported macOS target architecture"),
        };
        assert!(
            Command::new("xcrun")
                .args([
                    "clang",
                    "-arch",
                    architecture,
                    "-fobjc-arc",
                    "-c",
                    source,
                    "-o"
                ])
                .arg(&object)
                .status()
                .expect("Run macOS Objective-C compiler")
                .success()
        );
        assert!(
            Command::new("xcrun")
                .args(["ar", "rcs"])
                .arg(archive)
                .arg(object)
                .status()
                .expect("Archive macOS Bluetooth radio")
                .success()
        );
        println!("cargo:rerun-if-changed={source}");
        println!("cargo:rustc-link-search=native={}", out.display());
        println!("cargo:rustc-link-lib=static=plain_macos_ble");
        println!("cargo:rustc-link-lib=framework=CoreBluetooth");
    }
    tauri_build::build();
}
