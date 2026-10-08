pub mod default_model;
pub mod manifest;
mod onnx_package;
mod preprocess;
mod session;
use manifest::{Layout, Manifest, TextRole};
use session::{Input, Session};
use sha2::{Digest, Sha256};
use std::{
    ffi::CString,
    path::{Path, PathBuf},
    sync::Mutex,
};
use tokenizers::{PaddingParams, PaddingStrategy, Tokenizer, TruncationParams};

pub trait ImageEncoder: Send + Sync {
    fn release(&self) {}
    fn image(&self, path: &Path) -> Result<Option<Vec<f32>>, String>;
}
pub struct Engine {
    pub manifest: Manifest,
    directory: PathBuf,
    tokenizer: Tokenizer,
    image: Mutex<Option<Session>>,
    text: Mutex<Option<Session>>,
}
pub fn normalize(values: &mut [f32]) -> Result<(), String> {
    let norm = values
        .iter()
        .map(|v| (*v as f64).powi(2))
        .sum::<f64>()
        .sqrt();
    if values.is_empty()
        || values.iter().any(|v| !v.is_finite())
        || !norm.is_finite()
        || norm == 0.0
    {
        return Err("Invalid/zero image search embedding".into());
    }
    for value in values {
        *value = (*value as f64 / norm) as f32;
    }
    Ok(())
}
impl Engine {
    pub fn open(directory: PathBuf, manifest: Manifest) -> Result<Self, String> {
        manifest.validate()?;
        for file in &manifest.files {
            let path = directory.join(&file.name);
            if std::fs::symlink_metadata(&path)
                .map_err(|e| e.to_string())?
                .file_type()
                .is_symlink()
            {
                return Err("Model package symlinks are unsupported".into());
            }
            let mut source = std::fs::File::open(path).map_err(|e| e.to_string())?;
            if source.metadata().map_err(|e| e.to_string())?.len() != file.size {
                return Err(format!("Model size mismatch: {}", file.name));
            }
            let mut hash = Sha256::new();
            let mut buffer = [0u8; 65536];
            loop {
                let count =
                    std::io::Read::read(&mut source, &mut buffer).map_err(|e| e.to_string())?;
                if count == 0 {
                    break;
                }
                hash.update(&buffer[..count]);
            }
            if format!("{:x}", hash.finalize()) != file.sha256.to_lowercase() {
                return Err(format!("Model checksum mismatch: {}", file.name));
            }
        }
        onnx_package::validate(&directory.join(&manifest.image.file))?;
        if manifest.text.file != manifest.image.file {
            onnx_package::validate(&directory.join(&manifest.text.file))?;
        }
        let mut tokenizer = Tokenizer::from_file(directory.join(&manifest.tokenizer.file))
            .map_err(|e| e.to_string())?;
        let pad_id = tokenizer
            .token_to_id(&manifest.tokenizer.pad_token)
            .ok_or("Tokenizer pad token not found")?;
        tokenizer
            .with_truncation(Some(TruncationParams {
                max_length: manifest.tokenizer.context_length,
                ..Default::default()
            }))
            .map_err(|e| e.to_string())?;
        tokenizer.with_padding(Some(PaddingParams {
            strategy: PaddingStrategy::Fixed(manifest.tokenizer.context_length),
            pad_id,
            pad_token: manifest.tokenizer.pad_token.clone(),
            ..Default::default()
        }));
        let engine = Self {
            manifest,
            directory,
            tokenizer,
            image: Mutex::new(None),
            text: Mutex::new(None),
        };
        engine.image_tensor(vec![
            0.0;
            (engine.manifest.preprocess.size.pow(2) * 3) as usize
        ])?;
        engine.release_image();
        engine.text("image")?;
        Ok(engine)
    }
    pub fn relocate(mut self, directory: PathBuf) -> Self {
        self.directory = directory;
        self
    }
    fn session(&self, file: &str) -> Result<Session, String> {
        Session::new(&self.directory.join(file))
    }
    pub fn text(&self, text: &str) -> Result<Vec<f32>, String> {
        if text.len() > 65536 {
            return Err("Image search text exceeds limit".into());
        }
        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| e.to_string())?;
        if encoding.len() != self.manifest.tokenizer.context_length {
            return Err("Tokenizer context length mismatch".into());
        }
        let mut data = self
            .manifest
            .text
            .inputs
            .iter()
            .map(|input| {
                match input.role {
                    TextRole::Ids => encoding.get_ids(),
                    TextRole::AttentionMask => encoding.get_attention_mask(),
                    TextRole::TypeIds => encoding.get_type_ids(),
                }
                .iter()
                .map(|v| *v as i64)
                .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let names = self
            .manifest
            .text
            .inputs
            .iter()
            .map(|input| CString::new(input.name.as_str()).map_err(|e| e.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        let shape = [1, encoding.len() as i64];
        let mut inputs = data
            .iter_mut()
            .zip(&names)
            .map(|(data, name)| Input {
                name: name.as_ptr(),
                data: data.as_mut_ptr().cast(),
                bytes: data.len() * 8,
                shape: shape.as_ptr(),
                rank: 2,
                element_type: 7,
            })
            .collect::<Vec<_>>();
        let mut session = self.text.lock().map_err(|e| e.to_string())?;
        if session.is_none() {
            *session = Some(self.session(&self.manifest.text.file)?);
        }
        session.as_mut().unwrap().run(
            &mut inputs,
            &self.manifest.text.output,
            self.manifest.dimensions,
        )
    }
    fn image_tensor(&self, mut data: Vec<f32>) -> Result<Vec<f32>, String> {
        let size = self.manifest.preprocess.size as i64;
        let shape = match self.manifest.preprocess.layout {
            Layout::Nchw => [1, 3, size, size],
            Layout::Nhwc => [1, size, size, 3],
        };
        let name = CString::new(self.manifest.image.input.as_str()).map_err(|e| e.to_string())?;
        let mut inputs = [Input {
            name: name.as_ptr(),
            data: data.as_mut_ptr().cast(),
            bytes: data.len() * 4,
            shape: shape.as_ptr(),
            rank: 4,
            element_type: 1,
        }];
        let mut session = self.image.lock().map_err(|e| e.to_string())?;
        if session.is_none() {
            *session = Some(self.session(&self.manifest.image.file)?);
        }
        session.as_mut().unwrap().run(
            &mut inputs,
            &self.manifest.image.output,
            self.manifest.dimensions,
        )
    }
    pub fn release_all(&self) {
        self.release_image();
        *self.text.lock().unwrap() = None;
    }
    pub fn release_image(&self) {
        *self.image.lock().unwrap() = None;
    }
}
impl ImageEncoder for Engine {
    fn release(&self) {
        self.release_image();
    }
    fn image(&self, path: &Path) -> Result<Option<Vec<f32>>, String> {
        preprocess::tensor(path, &self.manifest.preprocess)?
            .map(|values| self.image_tensor(values))
            .transpose()
    }
}

#[cfg(test)]
#[path = "../../tests/unit/image_inference/mod.rs"]
pub(crate) mod tests;
