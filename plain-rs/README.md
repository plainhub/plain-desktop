# plain-rs

Shared Rust implementation for Plain projects. Consumers select Cargo features instead of copying protocol or cryptographic algorithms.

| Feature | Public API | Purpose |
| --- | --- | --- |
| `crypto` (default) | `plain_rs::crypto` | XChaCha20-Poly1305, ChaCha20-Poly1305, P-256 ECDH, Ed25519, randomness and hashes |
| `ble` | `plain_rs::ble` | Binary envelopes, bounded framing and assembly, gateway requests |
| `ble-native` | `plain_rs::ble::ffi` | Shared C ABI and Android JNI codec adapters; enables `ble` and `crypto` |

Protocol-only consumers set `default-features = false` and enable the features they use. These features do not enable `content_api`, HTTP servers, databases or media. The existing `content_api::ble_wire` export points to the same BLE implementation.

Hosts retain platform Bluetooth operations, permissions, persistence and application callbacks. Kotlin plain-common contains no BLE implementation. Mobile hosts use the same native bridge and keep only platform adaptation.

Validate with `cargo test -p plain-rs --no-default-features --features ble-native`, then build and test consumers. Fixed vectors cover framing boundaries, malformed messages and cryptographic interoperability; physical radio behavior requires device testing.
