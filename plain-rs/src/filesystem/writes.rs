use serde::Deserialize;
use std::{
    fs,
    io::{self, Write},
    path::Path,
};

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub enum Operation {
    CreateDirectory {
        path: String,
    },
    CreateFile {
        path: String,
    },
    WriteText {
        path: String,
        content: String,
        overwrite: bool,
    },
}
impl Operation {
    pub fn path(&self) -> &str {
        match self {
            Self::CreateDirectory { path }
            | Self::CreateFile { path }
            | Self::WriteText { path, .. } => path,
        }
    }
    pub fn apply(&self) -> io::Result<()> {
        let path = Path::new(self.path());
        if !path.is_absolute() || path.parent().is_none() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "absolute non-root path required",
            ));
        }
        match self {
            Self::CreateDirectory { .. } => fs::create_dir_all(path),
            Self::CreateFile { .. } => {
                match fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(path)
                {
                    Ok(file) => file.sync_all(),
                    Err(error)
                        if error.kind() == io::ErrorKind::AlreadyExists
                            && fs::metadata(path)?.is_file() =>
                    {
                        Ok(())
                    }
                    Err(error) => Err(error),
                }
            }
            Self::WriteText {
                content, overwrite, ..
            } => {
                match fs::metadata(path) {
                    Ok(metadata) if !metadata.is_file() => {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "regular file required",
                        ));
                    }
                    Ok(_) => {}
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error),
                }
                let mut options = fs::OpenOptions::new();
                options.write(true);
                if *overwrite {
                    options.create(true).truncate(true);
                } else {
                    options.create_new(true);
                }
                let mut file = options.open(path)?;
                file.write_all(content.as_bytes())?;
                file.sync_all()
            }
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/filesystem/writes.rs"]
mod tests;
