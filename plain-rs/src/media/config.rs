//! Lightweight TOML config loader. Mirrors the Go `internal/config` package
//! which only understands flat `key = value` pairs, dotted section headers
//! (`[foo]\n[foo.bar]`), and a small set of primitive types. We do not depend
//! on the `toml` crate for reads because the Go side does its own ad-hoc
//! parsing - we want byte-for-byte compatibility on `key1.key2` lookups.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

#[derive(Default, Debug, Clone)]
pub struct Config {
    pub values: BTreeMap<String, String>,
}

impl Config {
    pub fn load(path: &Path) -> Self {
        let Ok(content) = fs::read_to_string(path) else {
            return Self::default();
        };
        Self::parse(&content)
    }

    pub fn parse(content: &str) -> Self {
        let mut values = BTreeMap::new();
        let mut prefix: Vec<String> = Vec::new();
        for raw in content.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(rest) = line.strip_prefix('[') {
                if let Some(name) = rest.strip_suffix(']') {
                    prefix = name.split('.').map(|s| s.trim().to_string()).collect();
                    continue;
                }
            }
            if let Some((k, v)) = line.split_once('=') {
                let key = k.trim().to_string();
                let mut val = v.trim().to_string();
                // strip inline comment
                if let Some(idx) = val.find('#') {
                    val.truncate(idx);
                    val = val.trim().to_string();
                }
                // strip surrounding quotes
                if (val.starts_with('"') && val.ends_with('"') && val.len() >= 2)
                    || (val.starts_with('\'') && val.ends_with('\'') && val.len() >= 2)
                {
                    val = val[1..val.len() - 1].to_string();
                }
                let mut full_key = String::new();
                for p in &prefix {
                    full_key.push_str(p);
                    full_key.push('.');
                }
                full_key.push_str(&key);
                values.insert(full_key, val);
            }
        }
        Self { values }
    }

    pub fn get_string(&self, key: &str) -> String {
        self.values.get(key).cloned().unwrap_or_default()
    }

    pub fn get_int(&self, key: &str) -> i64 {
        self.values
            .get(key)
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
    }

    pub fn get_bool(&self, key: &str) -> bool {
        matches!(
            self.values.get(key).map(|s| s.as_str()),
            Some("true") | Some("1") | Some("yes") | Some("on")
        )
    }
}

#[cfg(test)]
#[path = "../../tests/unit/media/config.rs"]
mod tests;
