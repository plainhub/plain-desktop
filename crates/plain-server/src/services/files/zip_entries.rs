use sha2::{Digest, Sha256};
use std::{
    fs::File,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

pub(crate) fn extract(cache: &Path, reference: &str) -> Result<PathBuf, String> {
    let (archive, name) = reference
        .split_once("!zip!/")
        .ok_or("Archive entry reference required")?;
    if !Path::new(archive).is_absolute() || name.is_empty() {
        return Err("Absolute archive path and entry name required".into());
    }
    let file = File::open(archive).map_err(|error| error.to_string())?;
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|error| error.to_string())?;
    let mut entry = zip.by_name(name).map_err(|error| error.to_string())?;
    if entry.is_dir() {
        return Err("Regular archive entry required".into());
    }
    let mut hash = Sha256::new();
    hash.update(archive.as_bytes());
    hash.update(
        metadata
            .modified()
            .map_err(|error| error.to_string())?
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos()
            .to_le_bytes(),
    );
    hash.update(metadata.len().to_le_bytes());
    hash.update(name.as_bytes());
    hash.update(entry.crc32().to_le_bytes());
    let key = plain_rs::utils::hex::bytes_to_hex(&hash.finalize());
    let extension = Path::new(name)
        .extension()
        .and_then(|value| value.to_str())
        .filter(|value| value.chars().all(|ch| ch.is_ascii_alphanumeric()))
        .unwrap_or("bin");
    std::fs::create_dir_all(cache).map_err(|error| error.to_string())?;
    let output = cache.join(format!("{key}.{extension}"));
    if output
        .metadata()
        .is_ok_and(|meta| meta.is_file() && meta.len() == entry.size())
    {
        return Ok(output);
    }
    let temporary = cache.join(format!("{key}-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| -> Result<(), String> {
        let mut target = File::create(&temporary).map_err(|error| error.to_string())?;
        std::io::copy(&mut entry, &mut target).map_err(|error| error.to_string())?;
        target.sync_all().map_err(|error| error.to_string())?;
        std::fs::rename(&temporary, &output).map_err(|error| error.to_string())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result.map(|_| output)
}

#[cfg(test)]
#[path = "../../../tests/unit/services/files/zip_entries.rs"]
mod tests;
