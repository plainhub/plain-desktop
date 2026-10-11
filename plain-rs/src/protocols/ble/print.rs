//! Prints the Kotlin `BleUuids` source from the Rust-owned UUID constants.
//!
//! The Plain-owned GATT identifiers live here in Rust (single source of
//! truth). The App's `BleUuids.kt` is generated from this module so the two
//! sides can never drift. Regenerate with:
//!
//! ```text
//! cargo test -p plain-rs --features ble ble::print::regenerate_kotlin_uuids -- --ignored --nocapture
//! ```
//!
//! Output is byte-stable when the constants are unchanged; `guard` fails when
//! the committed Kotlin file goes stale.

use crate::protocols::ble::{CCC_DESCRIPTOR_UUID, HTTP_CHAR_UUID, NEARBY_CHAR_UUID, SERVICE_UUID};

/// The generated Kotlin `BleUuids` object, header included.
pub fn kotlin_uuids_source() -> String {
    format!(
        r#"// GENERATED from plain-rs protocols/ble UUID constants. Do not edit by hand.
// Regenerate: cargo test -p plain-rs --features ble ble::print::regenerate_kotlin_uuids -- --ignored --nocapture
package com.ismartcoding.plain.ble

object BleUuids {{
    const val SERVICE_UUID = "{SERVICE_UUID}"
    const val NEARBY_CHAR_UUID = "{NEARBY_CHAR_UUID}"
    const val HTTP_CHAR_UUID = "{HTTP_CHAR_UUID}"
    const val CCC_DESCRIPTOR_UUID = "{CCC_DESCRIPTOR_UUID}"
}}
"#
    )
}

#[cfg(test)]
mod tests {
    use super::kotlin_uuids_source;

    #[test]
    fn source_is_stable() {
        let first = kotlin_uuids_source();
        assert_eq!(first, kotlin_uuids_source());
        // Each constant must appear exactly once with the right value.
        assert!(first.contains("d8d5c4a0-8f0a-4e7d-b9e1-706c70616901"));
        assert!(first.contains("d8d5c4a0-8f0a-4e7d-b9e1-706c70616911"));
        assert!(first.contains("d8d5c4a0-8f0a-4e7d-b9e1-706c70616913"));
        assert!(first.contains("00002902-0000-1000-8000-00805f9b34fb"));
    }

    /// Writes the generated file to the App's `BleUuids.kt`. Ignored by default;
    /// run explicitly to regenerate.
    #[test]
    #[ignore = "generator; run explicitly to regenerate BleUuids.kt"]
    fn regenerate_kotlin_uuids() {
        let path = std::env::var("PLAIN_APP_BLE_UUIDS_PATH").unwrap_or_else(|_| {
            // Default to the sibling App checkout used in this workspace.
            concat!(
                "/Users/mac/Projects/smartcoding/plainhub/plain-app/shared/src/commonMain/",
                "kotlin/com/ismartcoding/plain/ble/BleUuids.kt"
            )
            .to_string()
        });
        std::fs::write(&path, kotlin_uuids_source()).unwrap();
        eprintln!("wrote {path}");
    }
}