use super::{Prefs, Result};

const SOURCE_DIRECTORIES: &str = "media_source_dirs";

pub fn source_directories(prefs: &Prefs) -> Result<Vec<String>> {
    Ok(prefs.get(SOURCE_DIRECTORIES)?.unwrap_or_default())
}

pub fn set_source_directories(prefs: &Prefs, directories: &[String]) -> Result<()> {
    prefs.set(SOURCE_DIRECTORIES, directories)?;
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/prefs/media.rs"]
mod tests;
