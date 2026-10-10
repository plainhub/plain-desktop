//! Test-only fixture: writes the tiny paired ONNX encoders plus their
//! manifest into `directory` and returns the manifest describing them.
//!
//! Exposed through the `test-seams` feature so dependent crates' test
//! binaries (plain-server's image-models tests) can build the same engine
//! package; `#[cfg(test)]` in this crate cannot reach them.

use crate::models::manifest;
use crate::models::manifest::Manifest;
use sha2::{Digest, Sha256};
use std::path::Path;

pub fn fixture(directory: &Path) -> Manifest {
    std::fs::create_dir_all(directory).unwrap();
    let mut manifest = super::default_model::manifest();
    let data: [(&str, &[u8]); 3] = [
        (
            "image.onnx",
            include_bytes!("../../../../plain-rs/testdata/image_search/image.onnx"),
        ),
        (
            "text.onnx",
            include_bytes!("../../../../plain-rs/testdata/image_search/text.onnx"),
        ),
        (
            "tokenizer.json",
            include_bytes!("../../../../plain-rs/testdata/image_search/tokenizer.json"),
        ),
    ];
    manifest.files = data
        .into_iter()
        .map(|(name, bytes)| {
            std::fs::write(directory.join(name), bytes).unwrap();
            manifest::File {
                name: name.into(),
                size: bytes.len() as u64,
                sha256: format!("{:x}", Sha256::digest(bytes)),
            }
        })
        .collect();
    manifest.dimensions = 4;
    manifest.image.input = "pixels".into();
    manifest.image.output = "embedding".into();
    manifest.text.inputs[0].name = "ids".into();
    manifest.text.output = "embedding".into();
    manifest.preprocess.size = 16;
    manifest.tokenizer.context_length = 4;
    manifest.tokenizer.pad_token = "[PAD]".into();
    std::fs::write(
        directory.join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    manifest
}
