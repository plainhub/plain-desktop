use super::manifest::*;

const BASE: &str = "https://huggingface.co/onnx-community/siglip2-base-patch16-256-ONNX/resolve/d1114256522a37ffa257a0a58017348ab0058db2";

pub fn manifest() -> Manifest {
    Manifest {
        format_version: 1,
        id: "siglip2-base-patch16-256".into(),
        name: "SigLIP2 Base".into(),
        license: "Apache-2.0".into(),
        source: "https://huggingface.co/google/siglip2-base-patch16-256".into(),
        dimensions: 768,
        minimum_score: 0.0,
        files: vec![
            File {
                name: "image.onnx".into(),
                size: 186131676,
                sha256: "fe9ad8020a6d3d98d394c9be8f07064066135fc2f87ec11692de0b677c0ac4db".into(),
            },
            File {
                name: "text.onnx".into(),
                size: 564862230,
                sha256: "80954edffdc689599e5d5bc6a1738380bc9e8139a18e5c8892485f248b6b4890".into(),
            },
            File {
                name: "tokenizer.json".into(),
                size: 34363039,
                sha256: "cb9140fae3ac5122c972d37adf83e1248471a38147ad76f8215c8872c6fd8322".into(),
            },
        ],
        image: ImageEncoder {
            file: "image.onnx".into(),
            input: "pixel_values".into(),
            output: "pooler_output".into(),
        },
        text: TextEncoder {
            file: "text.onnx".into(),
            inputs: vec![TextInput {
                name: "input_ids".into(),
                role: TextRole::Ids,
            }],
            output: "pooler_output".into(),
        },
        tokenizer: Tokenizer {
            file: "tokenizer.json".into(),
            context_length: 64,
            pad_token: "<pad>".into(),
        },
        preprocess: Preprocess {
            size: 256,
            resize: Resize::Stretch,
            layout: Layout::Nchw,
            interpolation: Interpolation::Bilinear,
            mean: [0.5; 3],
            std: [0.5; 3],
        },
    }
}
pub fn downloads(manifest: &Manifest) -> Vec<(&File, String)> {
    manifest
        .files
        .iter()
        .zip([
            "onnx/vision_model_fp16.onnx",
            "onnx/text_model_fp16.onnx",
            "tokenizer.json",
        ])
        .map(|(file, source)| (file, format!("{BASE}/{source}")))
        .collect()
}
