use super::*;
use sha2::{Digest, Sha256};

pub(crate) fn fixture(directory: &Path) -> Manifest {
    std::fs::create_dir_all(directory).unwrap();
    let mut manifest = default_model::manifest();
    let data: [(&str, &[u8]); 3] = [
        (
            "image.onnx",
            include_bytes!("../../../testdata/image_search/image.onnx"),
        ),
        (
            "text.onnx",
            include_bytes!("../../../testdata/image_search/text.onnx"),
        ),
        (
            "tokenizer.json",
            include_bytes!("../../../testdata/image_search/tokenizer.json"),
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
#[test]
fn real_runtime_executes_paired_encoders_and_reloads_released_sessions() {
    let directory = tempfile::tempdir().unwrap();
    let engine = Engine::open(directory.path().into(), fixture(directory.path())).unwrap();
    let path = directory.path().join("photo.png");
    image::RgbImage::from_pixel(64, 64, image::Rgb([128, 64, 32]))
        .save(&path)
        .unwrap();
    assert_eq!(engine.text("cat").unwrap(), vec![1.0, 0.0, 0.0, 0.0]);
    assert_eq!(
        engine.image(&path).unwrap().unwrap(),
        vec![1.0, 0.0, 0.0, 0.0]
    );
    let ids = engine.tokenizer.encode("cat", true).unwrap();
    assert_eq!(ids.get_ids(), &[3, 0, 0, 0]);
    assert_eq!(
        engine
            .tokenizer
            .encode("cat cat cat cat cat", true)
            .unwrap()
            .len(),
        4
    );
    engine.release_all();
    assert_eq!(engine.text("image").unwrap()[0], 1.0);
    assert_eq!(engine.image(&path).unwrap().unwrap()[0], 1.0);
}
#[test]
fn packages_reject_corruption_wrong_shapes_and_path_traversal() {
    let directory = tempfile::tempdir().unwrap();
    let mut manifest = fixture(directory.path());
    manifest.files[0].name = "../image.onnx".into();
    assert!(manifest.validate().is_err());
    manifest = fixture(directory.path());
    manifest.dimensions = 5;
    assert!(
        Engine::open(directory.path().into(), manifest)
            .err()
            .unwrap()
            .contains("embedding")
    );
    manifest = fixture(directory.path());
    std::fs::write(
        directory.path().join("image.onnx"),
        vec![0; manifest.files[0].size as usize],
    )
    .unwrap();
    assert!(
        Engine::open(directory.path().into(), manifest)
            .err()
            .unwrap()
            .contains("checksum")
    );
}
#[test]
fn external_tensor_files_are_rejected_before_loading() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("external.onnx");
    std::fs::write(&path, [0x3a, 4, 0x2a, 2, 0x6a, 0]).unwrap();
    assert!(
        onnx_package::validate(&path)
            .unwrap_err()
            .contains("External")
    );
    std::fs::write(&path, [0x3a, 255]).unwrap();
    assert!(onnx_package::validate(&path).is_err());
}
#[test]
fn normalization_handles_extreme_floats_and_rejects_invalid_embeddings() {
    let mut vector = [f32::MAX, f32::MAX];
    normalize(&mut vector).unwrap();
    assert!((vector[0] - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6);
    for mut values in [vec![0.0], vec![f32::NAN], vec![f32::INFINITY], vec![]] {
        assert!(normalize(&mut values).is_err());
    }
}
#[test]
fn preprocessing_uses_manifest_normalization_and_skips_tiny_images() {
    let directory = tempfile::tempdir().unwrap();
    let config = default_model::manifest().preprocess;
    let path = directory.path().join("photo.png");
    image::RgbImage::from_pixel(64, 64, image::Rgb([255, 0, 128]))
        .save(&path)
        .unwrap();
    let values = preprocess::tensor(&path, &config).unwrap().unwrap();
    let plane = config.size.pow(2) as usize;
    assert_eq!(values[0], 1.0);
    assert_eq!(values[plane], -1.0);
    assert!((values[2 * plane] - 1.0 / 255.0).abs() < 1e-6);
    image::RgbImage::new(32, 32).save(&path).unwrap();
    assert!(preprocess::tensor(&path, &config).unwrap().is_none());
}

#[test]
fn manifest_reader_bounds_input_before_parsing() {
    let directory = tempfile::tempdir().unwrap();
    fixture(directory.path());
    let path = directory.path().join("manifest.json");
    assert!(Manifest::read(&path).is_ok());
    std::fs::write(&path, vec![b' '; 65537]).unwrap();
    assert!(Manifest::read(&path).err().unwrap().contains("64 KiB"));
}
