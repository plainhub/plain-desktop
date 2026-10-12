use std::path::{Path, PathBuf};

pub(crate) struct Policy {
    pub cache_directory: PathBuf,
    pub excluded_roots: Vec<String>,
}

impl Policy {
    pub(crate) fn load(cache_directory: &Path, config_file: Option<&Path>) -> Result<Self, String> {
        if !cache_directory.is_absolute() {
            return Err("Absolute media cache directory required".into());
        }
        let config = match config_file {
            Some(path) => {
                if !path.is_absolute() {
                    return Err("Absolute media configuration path required".into());
                }
                match std::fs::read_to_string(path) {
                    Ok(text) => plain_media::config::Config::parse(&text),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        Default::default()
                    }
                    Err(error) => return Err(error.to_string()),
                }
            }
            None => Default::default(),
        };
        let mut excluded_roots = config
            .get_string("media_scan.excluded_dirs")
            .split(',')
            .map(str::trim)
            .filter(|root| !root.is_empty())
            .map(|root| {
                if !Path::new(root).is_absolute() {
                    return Err("Absolute media excluded directory required".into());
                }
                Ok(root.to_owned())
            })
            .collect::<Result<Vec<_>, String>>()?;
        let real_roots: Vec<String> = excluded_roots
            .iter()
            .filter_map(|root| {
                std::fs::canonicalize(root)
                    .ok()
                    .and_then(|path| path.to_str().map(str::to_owned))
            })
            .collect();
        for root in real_roots {
            if !excluded_roots.contains(&root) {
                excluded_roots.push(root);
            }
        }
        let cache_directory = match cache_directory.canonicalize() {
            Ok(path) => path,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                cache_directory.to_path_buf()
            }
            Err(error) => return Err(error.to_string()),
        };
        Ok(Self {
            cache_directory,
            excluded_roots,
        })
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/services/files/media_scan_policy.rs"]
mod tests;
