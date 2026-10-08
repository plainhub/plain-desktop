use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    path::{Component, Path},
};

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct File {
    pub name: String,
    pub size: u64,
    pub sha256: String,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImageEncoder {
    pub file: String,
    pub input: String,
    pub output: String,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextEncoder {
    pub file: String,
    pub inputs: Vec<TextInput>,
    pub output: String,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextInput {
    pub name: String,
    pub role: TextRole,
}
#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TextRole {
    Ids,
    AttentionMask,
    TypeIds,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Tokenizer {
    pub file: String,
    pub context_length: usize,
    pub pad_token: String,
}
#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Resize {
    CenterCrop,
    Stretch,
}
#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Layout {
    Nchw,
    Nhwc,
}
#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Interpolation {
    Bilinear,
    Bicubic,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Preprocess {
    pub size: u32,
    pub resize: Resize,
    pub layout: Layout,
    pub interpolation: Interpolation,
    pub mean: [f32; 3],
    pub std: [f32; 3],
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    pub format_version: u32,
    pub id: String,
    pub name: String,
    pub license: String,
    pub source: String,
    pub dimensions: usize,
    pub minimum_score: f32,
    pub files: Vec<File>,
    pub image: ImageEncoder,
    pub text: TextEncoder,
    pub tokenizer: Tokenizer,
    pub preprocess: Preprocess,
}
impl Manifest {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > 64 * 1024 {
            return Err("Model manifest exceeds 64 KiB".into());
        }
        let value: Self = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        value.validate()?;
        Ok(value)
    }
    pub fn validate(&self) -> Result<(), String> {
        let valid_name = |name: &str| {
            matches!(
                Path::new(name).components().collect::<Vec<_>>().as_slice(),
                [Component::Normal(_)]
            ) && name != "."
                && !name.contains(['/', '\\', '\0'])
        };
        if self.format_version != 1
            || self.id.is_empty()
            || self.name.is_empty()
            || self.license.is_empty()
            || self.source.is_empty()
            || !(1..=4096).contains(&self.dimensions)
            || !(16..=1024).contains(&self.preprocess.size)
            || !(2..=512).contains(&self.tokenizer.context_length)
            || !self.minimum_score.is_finite()
            || !(-1.0..=1.0).contains(&self.minimum_score)
            || self.files.is_empty()
            || self.files.len() > 4
            || self.preprocess.mean.iter().any(|v| !v.is_finite())
            || self
                .preprocess
                .std
                .iter()
                .any(|v| !v.is_finite() || *v <= 0.0)
        {
            return Err("Invalid image search model manifest".into());
        }
        let mut names = HashSet::new();
        for file in &self.files {
            if !valid_name(&file.name)
                || file.name == "manifest.json"
                || !names.insert(file.name.as_str())
                || file.size == 0
                || file.size > 2 * 1024 * 1024 * 1024
                || file.sha256.len() != 64
                || !file.sha256.bytes().all(|b| b.is_ascii_hexdigit())
            {
                return Err("Invalid model file declaration".into());
            }
        }
        let required = [&self.image.file, &self.text.file, &self.tokenizer.file];
        if required.iter().any(|name| !names.contains(name.as_str()))
            || names
                .iter()
                .any(|name| !required.iter().any(|required| required.as_str() == *name))
            || self.tokenizer.file == self.image.file
            || self.tokenizer.file == self.text.file
            || [&self.image.input, &self.image.output, &self.text.output]
                .iter()
                .any(|name| name.is_empty() || name.contains('\0'))
            || self.text.inputs.is_empty()
            || self.text.inputs.len() > 3
            || self.tokenizer.pad_token.is_empty()
        {
            return Err("Incomplete encoder/tokenizer model contract".into());
        }
        if self
            .files
            .iter()
            .any(|file| file.name == self.tokenizer.file && file.size > 64 * 1024 * 1024)
        {
            return Err("Tokenizer exceeds memory limit".into());
        }
        let mut inputs = HashSet::new();
        if self.text.inputs.iter().any(|input| {
            input.name.is_empty() || input.name.contains('\0') || !inputs.insert(&input.name)
        }) || self
            .text
            .inputs
            .iter()
            .filter(|input| matches!(input.role, TextRole::Ids))
            .count()
            != 1
        {
            return Err("Invalid text encoder inputs".into());
        }
        Ok(())
    }
    pub fn fingerprint(&self) -> Result<String, String> {
        use sha2::{Digest, Sha256};
        let bytes = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        Ok(format!("{:x}", Sha256::digest(bytes)))
    }
    pub fn size(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }
}
