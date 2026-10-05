use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
};
#[derive(Clone, Deserialize, Serialize)]
pub struct Failure {
    pub path: String,
    pub error: String,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub removed: bool,
    pub paths: Vec<String>,
    pub failures: Vec<Failure>,
}
pub struct Plan {
    root: PathBuf,
    leaves: Vec<PathBuf>,
    directories: Vec<PathBuf>,
    failures: Vec<Failure>,
    pub files: Vec<String>,
    identities: std::collections::HashMap<PathBuf, Identity>,
}
#[derive(PartialEq)]
struct Identity {
    device: u64,
    inode: u64,
    kind: u8,
}
impl Identity {
    fn of(meta: &fs::Metadata) -> Self {
        #[cfg(unix)]
        let (device, inode) = {
            use std::os::unix::fs::MetadataExt;
            (meta.dev(), meta.ino())
        };
        #[cfg(not(unix))]
        let (device, inode) = (
            0,
            meta.created()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(meta.len()),
        );
        Self {
            device,
            inode,
            kind: if meta.is_dir() {
                0
            } else if meta.is_file() {
                1
            } else {
                2
            },
        }
    }
}
pub fn validate(root: &Path) -> io::Result<()> {
    if !root.is_absolute()
        || root.parent().is_none()
        || matches!(
            root.to_string_lossy()
                .trim_end_matches('/')
                .rsplit('/')
                .next(),
            Some("." | "..")
        )
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "absolute non-root path required",
        ));
    }
    Ok(())
}
impl Plan {
    pub fn inspect(root: &Path) -> io::Result<Self> {
        validate(root)?;
        let mut plan = Self {
            root: root.to_owned(),
            leaves: Vec::new(),
            directories: Vec::new(),
            failures: Vec::new(),
            files: Vec::new(),
            identities: Default::default(),
        };
        let mut stack = vec![root.to_owned()];
        while let Some(path) = stack.pop() {
            let meta = match fs::symlink_metadata(&path) {
                Ok(meta) => meta,
                Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                Err(e) => {
                    plan.fail(&path, e);
                    continue;
                }
            };
            plan.identities.insert(path.clone(), Identity::of(&meta));
            if meta.is_dir() {
                plan.directories.push(path.clone());
                match fs::read_dir(&path) {
                    Ok(entries) => {
                        for entry in entries {
                            match entry {
                                Ok(entry) => stack.push(entry.path()),
                                Err(e) => plan.fail(&path, e),
                            }
                        }
                    }
                    Err(e) => plan.fail(&path, e),
                }
            } else {
                if meta.is_file() {
                    plan.files.push(path.to_string_lossy().into_owned());
                }
                plan.leaves.push(path);
            }
        }
        Ok(plan)
    }
    fn fail(&mut self, path: &Path, error: io::Error) {
        self.failures.push(Failure {
            path: path.to_string_lossy().into_owned(),
            error: error.to_string(),
        });
    }
    pub fn paths(&self) -> Vec<String> {
        self.leaves
            .iter()
            .chain(&self.directories)
            .map(|p| p.to_string_lossy().into_owned())
            .collect()
    }
    fn unchanged(&self, path: &Path) -> io::Result<()> {
        let current = Identity::of(&fs::symlink_metadata(path)?);
        if self.identities.get(path) != Some(&current) {
            return Err(io::Error::other("deletion target changed"));
        }
        Ok(())
    }
    pub fn execute(mut self) -> Outcome {
        let mut paths = Vec::new();
        for path in std::mem::take(&mut self.leaves) {
            match self.unchanged(&path).and_then(|_| fs::remove_file(&path)) {
                Ok(()) => paths.push(path.to_string_lossy().into_owned()),
                Err(e) => self.fail(&path, e),
            }
        }
        for path in std::mem::take(&mut self.directories).into_iter().rev() {
            match self.unchanged(&path).and_then(|_| fs::remove_dir(&path)) {
                Ok(()) => paths.push(path.to_string_lossy().into_owned()),
                Err(e) => self.fail(&path, e),
            }
        }
        Outcome {
            removed: paths.iter().any(|p| Path::new(p) == self.root),
            paths,
            failures: self.failures,
        }
    }
}
#[cfg(test)]
#[path = "../../tests/unit/filesystem/deletion.rs"]
mod tests;
