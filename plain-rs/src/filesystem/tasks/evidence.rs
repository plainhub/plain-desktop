use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    entries: Vec<Entry>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Entry {
    path: PathBuf,
    kind: u8,
    size: u64,
    modified: Option<std::time::SystemTime>,
    identity: (u64, u64),
    digest: Vec<u8>,
}

pub async fn capture(root: PathBuf) -> Result<Evidence> {
    tokio::task::spawn_blocking(move || inspect(&root, true)).await?
}
fn identity(meta: &fs::Metadata) -> (u64, u64) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        (meta.dev(), meta.ino())
    }
    #[cfg(not(unix))]
    {
        (0, 0)
    }
}
fn inspect(root: &Path, content: bool) -> Result<Evidence> {
    let mut stack = vec![root.to_owned()];
    let mut entries = Vec::new();
    while let Some(path) = stack.pop() {
        let before = fs::symlink_metadata(&path)?;
        let kind = if before.is_dir() {
            0
        } else if before.is_file() {
            1
        } else if before.file_type().is_symlink() {
            2
        } else {
            bail!("unsupported file recovery type")
        };
        let digest = match kind {
            0 => {
                for child in fs::read_dir(&path)? {
                    stack.push(child?.path());
                }
                Vec::new()
            }
            1 if content => {
                let mut file = fs::File::open(&path)?;
                if identity(&file.metadata()?) != identity(&before) {
                    bail!("file recovery source changed");
                }
                let mut hash = Sha256::new();
                let mut buffer = [0; 64 * 1024];
                loop {
                    let length = file.read(&mut buffer)?;
                    if length == 0 {
                        break;
                    }
                    hash.update(&buffer[..length]);
                }
                hash.finalize().to_vec()
            }
            1 => Vec::new(),
            _ => fs::read_link(&path)?
                .as_os_str()
                .as_encoded_bytes()
                .to_vec(),
        };
        let after = fs::symlink_metadata(&path)?;
        if before.len() != after.len()
            || before.modified().ok() != after.modified().ok()
            || identity(&before) != identity(&after)
        {
            bail!("file changed while recording recovery evidence");
        }
        entries.push(Entry {
            path: path
                .strip_prefix(root)
                .map_err(|_| anyhow!("invalid recovery path"))?
                .to_owned(),
            kind,
            size: if kind == 0 { 0 } else { after.len() },
            modified: if kind == 0 {
                None
            } else {
                after.modified().ok()
            },
            identity: identity(&after),
            digest,
        });
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(Evidence { entries })
}

pub async fn verify(root: PathBuf, expected: Evidence, content: bool) -> Result<()> {
    tokio::task::spawn_blocking(move || verify_sync(&root, &expected, content)).await?
}
pub fn verify_sync(root: &Path, expected: &Evidence, content: bool) -> Result<()> {
    let actual = inspect(root, content)?;
    let mut expected = expected.clone();
    if !content {
        for entry in &mut expected.entries {
            if entry.kind == 1 {
                entry.digest.clear();
            }
        }
    }
    if actual != expected {
        bail!("file recovery destination changed");
    }
    Ok(())
}
