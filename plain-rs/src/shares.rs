use crate::{
    db::{Db, ShareRow},
    prefs::Prefs,
};
use base64::{
    Engine,
    engine::general_purpose::{STANDARD, URL_SAFE},
};
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    path::{Component, Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Debug, Serialize, Deserialize, async_graphql::SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct Root {
    pub virtual_path: String,
    pub real_path: String,
    pub is_dir: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileParams {
    shared_id: String,
    virtual_path: String,
}

pub struct Service {
    db: Arc<Db>,
    prefs: Arc<Prefs>,
}
impl Service {
    pub fn new(db: Arc<Db>, prefs: Arc<Prefs>) -> Arc<Self> {
        Arc::new(Self { db, prefs })
    }
    pub fn roots(row: &ShareRow) -> anyhow::Result<Vec<Root>> {
        Ok(serde_json::from_str(&row.data)?)
    }
    fn build_roots(paths: Vec<String>) -> anyhow::Result<String> {
        anyhow::ensure!(!paths.is_empty(), "share needs at least one root");
        let mut seen = HashSet::new();
        let mut names = HashSet::new();
        let mut roots = Vec::new();
        for path in paths {
            let path = std::fs::canonicalize(path)?;
            if !seen.insert(path.clone()) {
                continue;
            }
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| anyhow::anyhow!("invalid root name"))?;
            anyhow::ensure!(names.insert(name.to_owned()), "duplicate root name: {name}");
            let meta = path.metadata()?;
            anyhow::ensure!(meta.is_dir() || meta.is_file(), "unsupported share root");
            roots.push(Root {
                virtual_path: format!("{name}{}", if meta.is_dir() { "/" } else { "" }),
                real_path: path
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("invalid root path"))?
                    .to_owned(),
                is_dir: meta.is_dir(),
            });
        }
        Ok(serde_json::to_string(&roots)?)
    }
    pub fn create(
        &self,
        name: String,
        paths: Vec<String>,
        token: String,
        read_only: bool,
        expires: Option<String>,
    ) -> anyhow::Result<ShareRow> {
        anyhow::ensure!(STANDARD.decode(&token)?.len() == 32, "invalid URL token");
        validate_expiry(expires.as_deref())?;
        let now = chrono::Utc::now().to_rfc3339();
        let row = ShareRow {
            id: crate::short_uuid::short_uuid(),
            name,
            password: String::new(),
            url_token: token,
            read_only,
            expires_at: expires,
            data: Self::build_roots(paths)?,
            created_at: now.clone(),
            updated_at: now,
        };
        self.db.share_save(&row)?;
        Ok(row)
    }
    pub fn update(
        &self,
        id: &str,
        name: &str,
        expires: Option<String>,
        paths: Option<Vec<String>>,
    ) -> anyhow::Result<ShareRow> {
        validate_expiry(expires.as_deref())?;
        let data = paths.map(Self::build_roots).transpose()?;
        anyhow::ensure!(
            self.db.share_update(
                id,
                name,
                expires.as_deref(),
                data.as_deref(),
                &chrono::Utc::now().to_rfc3339()
            )?,
            "share not found"
        );
        self.db
            .share_get(id)?
            .ok_or_else(|| anyhow::anyhow!("share not found"))
    }
    pub fn active(&self, id: &str, guest: bool) -> anyhow::Result<Option<ShareRow>> {
        if guest && !self.prefs.get_user::<bool>("service")?.unwrap_or(false) {
            return Ok(None);
        }
        Ok(self
            .db
            .share_get(id)?
            .filter(|r| match r.expires_at.as_deref() {
                None => true,
                Some(t) => {
                    chrono::DateTime::parse_from_rfc3339(t).is_ok_and(|t| t > chrono::Utc::now())
                }
            }))
    }
    pub fn token(&self, id: &str) -> anyhow::Result<String> {
        anyhow::ensure!(self.db.share_get(id)?.is_some(), "share not found");
        let secret = self
            .prefs
            .get::<String>("master_secret")?
            .ok_or_else(|| anyhow::anyhow!("master secret not initialized"))?;
        let key = STANDARD.decode(secret)?;
        anyhow::ensure!(key.len() == 32, "invalid master secret");
        let mut mac = Hmac::<sha2_mac::Sha256>::new_from_slice(&key)?;
        mac.update(id.as_bytes());
        Ok(URL_SAFE.encode(mac.finalize().into_bytes()))
    }
    pub fn resolve(
        &self,
        id: &str,
        virtual_path: &str,
        guest: bool,
    ) -> anyhow::Result<Option<String>> {
        let Some(row) = self.active(id, guest)? else {
            return Ok(None);
        };
        Self::resolve_row(&row, virtual_path)
    }
    fn resolve_row(row: &ShareRow, virtual_path: &str) -> anyhow::Result<Option<String>> {
        let roots = Self::roots(row)?;
        let normalized = virtual_path.trim().trim_start_matches('/');
        let (top, rest) = normalized.split_once('/').unwrap_or((normalized, ""));
        let root = if normalized.is_empty() {
            roots.first()
        } else {
            roots
                .iter()
                .find(|r| r.virtual_path.trim_end_matches('/') == top)
        };
        let Some(root) = root else { return Ok(None) };
        let base = Path::new(&root.real_path);
        if !root.is_dir && !rest.is_empty() {
            return Ok(None);
        };
        let mut relative = PathBuf::new();
        for part in Path::new(rest).components() {
            match part {
                Component::Normal(p) => relative.push(p),
                Component::CurDir => {}
                Component::ParentDir => {
                    if !relative.pop() {
                        return Ok(None);
                    }
                }
                _ => return Ok(None),
            }
        }
        let candidate = match std::fs::canonicalize(base.join(relative)) {
            Ok(p) => p,
            Err(_) => return Ok(None),
        };
        // Keep the persisted canonical root as authority even if the root is replaced by a symlink.
        if !candidate.starts_with(base) {
            return Ok(None);
        };
        Ok(candidate.to_str().map(str::to_owned))
    }
    pub fn browse(&self, id: &str, virtual_path: &str) -> anyhow::Result<(ShareRow, Vec<Root>)> {
        let row = self
            .active(id, true)?
            .ok_or_else(|| anyhow::anyhow!("share inactive"))?;
        let mut entries = Vec::new();
        if virtual_path.trim().trim_matches('/').is_empty() {
            for root in Self::roots(&row)? {
                if Self::resolve_row(&row, &root.virtual_path)?.is_some() {
                    entries.push(root);
                }
            }
        } else {
            let path = Self::resolve_row(&row, virtual_path)?
                .ok_or_else(|| anyhow::anyhow!("path not allowed"))?;
            if Path::new(&path).is_dir() {
                let parent = virtual_path.trim().trim_matches('/');
                for entry in std::fs::read_dir(path)? {
                    let entry = entry?;
                    let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                        continue;
                    };
                    if name.starts_with('.') {
                        continue;
                    }
                    let virtual_path = format!("{parent}/{name}");
                    let Some(real_path) = Self::resolve_row(&row, &virtual_path)? else {
                        continue;
                    };
                    let meta = Path::new(&real_path).metadata()?;
                    if !meta.is_dir() && !meta.is_file() {
                        continue;
                    }
                    entries.push(Root {
                        virtual_path,
                        real_path,
                        is_dir: meta.is_dir(),
                    });
                }
                entries.sort_by(|a, b| a.virtual_path.cmp(&b.virtual_path));
            }
        }
        Ok((row, entries))
    }

    pub fn archive(&self, id: &str, encrypted: &str) -> anyhow::Result<Vec<Root>> {
        let path = self
            .resolve_file(id, encrypted)?
            .ok_or_else(|| anyhow::anyhow!("share path not allowed"))?;
        let base = PathBuf::from(path);
        anyhow::ensure!(base.is_dir(), "shared path is not a directory");
        let mut pending = vec![base.clone()];
        let mut result = Vec::new();
        while let Some(dir) = pending.pop() {
            anyhow::ensure!(self.active(id, true)?.is_some(), "share inactive");
            for entry in std::fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                let meta = path.symlink_metadata()?;
                if meta.file_type().is_symlink() {
                    continue;
                }
                let canonical = std::fs::canonicalize(&path)?;
                anyhow::ensure!(canonical.starts_with(&base), "archive path outside root");
                if meta.is_dir() {
                    anyhow::ensure!(result.len() < 100_000, "too many archive entries");
                    result.push(Root {
                        virtual_path: format!(
                            "{}/",
                            path.strip_prefix(&base)?
                                .to_str()
                                .ok_or_else(|| anyhow::anyhow!("invalid entry name"))?
                        ),
                        real_path: canonical
                            .to_str()
                            .ok_or_else(|| anyhow::anyhow!("invalid entry path"))?
                            .into(),
                        is_dir: true,
                    });
                    pending.push(canonical);
                } else if meta.is_file() {
                    anyhow::ensure!(result.len() < 100_000, "too many archive files");
                    result.push(Root {
                        virtual_path: path
                            .strip_prefix(&base)?
                            .to_str()
                            .ok_or_else(|| anyhow::anyhow!("invalid entry name"))?
                            .into(),
                        real_path: canonical
                            .to_str()
                            .ok_or_else(|| anyhow::anyhow!("invalid entry path"))?
                            .into(),
                        is_dir: false,
                    });
                }
            }
        }
        result.sort_by(|a, b| a.virtual_path.cmp(&b.virtual_path));
        Ok(result)
    }

    pub fn resolve_file(&self, id: &str, encrypted: &str) -> anyhow::Result<Option<String>> {
        let Some(row) = self.active(id, true)? else {
            return Ok(None);
        };
        let ciphertext = crate::base64_decode(encrypted);
        let key = STANDARD.decode(&row.url_token)?;
        let Some(plain) = crate::xchacha_decrypt_raw(&key, &ciphertext) else {
            return Ok(None);
        };
        let params: FileParams = match serde_json::from_slice(&plain) {
            Ok(p) => p,
            Err(_) => return Ok(None),
        };
        if params.shared_id != id {
            return Ok(None);
        };
        Self::resolve_row(&row, &params.virtual_path)
    }
}
fn validate_expiry(value: Option<&str>) -> anyhow::Result<()> {
    if let Some(v) = value {
        chrono::DateTime::parse_from_rfc3339(v)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, Arc<Service>) {
        let dir = tempfile::tempdir().unwrap();
        let db = Arc::new(Db::open(&dir.path().join("db")).unwrap());
        let prefs = Arc::new(
            Prefs::load_pair(&dir.path().join("system"), &dir.path().join("user")).unwrap(),
        );
        prefs
            .set("master_secret", STANDARD.encode([7; 32]))
            .unwrap();
        prefs.set_user("service", true).unwrap();
        (dir, Service::new(db, prefs))
    }
    fn create(service: &Service, paths: Vec<PathBuf>) -> ShareRow {
        service
            .create(
                "share".into(),
                paths.iter().map(|p| p.to_str().unwrap().into()).collect(),
                STANDARD.encode([8; 32]),
                true,
                None,
            )
            .unwrap()
    }
    #[test]
    fn roots_traversal_and_files() {
        let (dir, s) = fixture();
        let root = dir.path().join("photos");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("a.jpg"), b"synthetic").unwrap();
        let file = dir.path().join("single.txt");
        std::fs::write(&file, b"text").unwrap();
        let row = create(&s, vec![root.clone(), root.clone(), file.clone()]);
        assert_eq!(Service::roots(&row).unwrap().len(), 2);
        assert_eq!(
            s.resolve(&row.id, "photos/./a.jpg", true).unwrap(),
            Some(
                std::fs::canonicalize(root.join("a.jpg"))
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .into()
            )
        );
        for p in [
            "photos/../single.txt",
            "photos/../../etc/passwd",
            "single.txt/a",
            "unknown/a",
        ] {
            assert!(s.resolve(&row.id, p, true).unwrap().is_none());
        }
        assert!(s.resolve(&row.id, "single.txt", true).unwrap().is_some());
    }
    #[test]
    fn duplicate_names_are_rejected_without_persisting() {
        let (dir, s) = fixture();
        let a = dir.path().join("a/docs");
        let b = dir.path().join("b/docs");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        assert!(
            s.create(
                "x".into(),
                vec![a.to_str().unwrap().into(), b.to_str().unwrap().into()],
                STANDARD.encode([8; 32]),
                true,
                None
            )
            .is_err()
        );
        assert!(s.db.share_list().unwrap().is_empty());
    }
    #[test]
    fn fresh_auth_revocation_expiry_and_disabled_service() {
        let (dir, s) = fixture();
        let a = dir.path().join("a");
        std::fs::create_dir(&a).unwrap();
        let row = create(&s, vec![a]);
        assert!(s.active(&row.id, true).unwrap().is_some());
        let token = s.token(&row.id).unwrap();
        assert_eq!(URL_SAFE.decode(token).unwrap().len(), 32);
        s.prefs.set_user("service", false).unwrap();
        assert!(s.active(&row.id, true).unwrap().is_none());
        s.prefs.set_user("service", true).unwrap();
        s.update(
            &row.id,
            "expired",
            Some("2000-01-01T00:00:00Z".into()),
            None,
        )
        .unwrap();
        assert!(s.active(&row.id, true).unwrap().is_none());
        s.update(&row.id, "active", None, None).unwrap();
        assert!(s.active(&row.id, true).unwrap().is_some());
        s.db.share_delete(&row.id).unwrap();
        assert!(s.active(&row.id, true).unwrap().is_none());
        assert!(s.update(&row.id, "revive", None, None).is_err());
    }
    #[test]
    fn encrypted_file_ids_are_share_bound_and_roots_are_fresh() {
        let (dir, s) = fixture();
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        std::fs::create_dir(&a).unwrap();
        std::fs::create_dir(&b).unwrap();
        std::fs::write(a.join("x"), b"x").unwrap();
        let row = create(&s, vec![a]);
        let encrypt = |id: &str| {
            STANDARD.encode(
                crate::xchacha_encrypt_raw(
                    &[8; 32],
                    serde_json::json!({"sharedId":id,"virtualPath":"a/x"})
                        .to_string()
                        .as_bytes(),
                )
                .unwrap(),
            )
        };
        let encoded = encrypt(&row.id);
        assert!(s.resolve_file(&row.id, &encoded).unwrap().is_some());
        assert!(
            s.resolve_file(&row.id, &encrypt("another-share"))
                .unwrap()
                .is_none()
        );
        assert!(s.resolve_file(&row.id, "invalid").unwrap().is_none());
        s.update(&row.id, "new", None, Some(vec![b.to_str().unwrap().into()]))
            .unwrap();
        assert!(s.resolve_file(&row.id, &encoded).unwrap().is_none());
        let restarted = Service::new(
            Arc::new(Db::open(&dir.path().join("db")).unwrap()),
            s.prefs.clone(),
        );
        assert_eq!(
            restarted.db.share_get(&row.id).unwrap().unwrap().name,
            "new"
        );
    }
    #[cfg(unix)]
    #[test]
    fn symlink_escape_and_root_replacement_are_rejected() {
        use std::os::unix::fs::symlink;
        let (dir, s) = fixture();
        let root = dir.path().join("root");
        let outside = dir.path().join("outside");
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join("secret"), b"private").unwrap();
        symlink(&outside, root.join("escape")).unwrap();
        let row = create(&s, vec![root.clone()]);
        assert!(
            s.resolve(&row.id, "root/escape/secret", true)
                .unwrap()
                .is_none()
        );
        std::fs::rename(&root, dir.path().join("old")).unwrap();
        symlink(&outside, &root).unwrap();
        assert!(s.resolve(&row.id, "root/secret", true).unwrap().is_none());
    }
    #[cfg(unix)]
    #[test]
    fn browse_and_archive_exclude_escaping_symlinks() {
        use std::os::unix::fs::symlink;
        let (dir, s) = fixture();
        let root = dir.path().join("root");
        std::fs::create_dir(&root).unwrap();
        let outside = dir.path().join("secret");
        std::fs::write(&outside, b"private").unwrap();
        std::fs::write(root.join("a"), b"allowed").unwrap();
        std::fs::create_dir(root.join("empty")).unwrap();
        symlink(outside, root.join("escape")).unwrap();
        let row = create(&s, vec![root]);
        let (_, entries) = s.browse(&row.id, "root").unwrap();
        assert_eq!(
            entries
                .iter()
                .map(|e| e.virtual_path.as_str())
                .collect::<Vec<_>>(),
            vec!["root/a", "root/empty"]
        );
        let encrypted = STANDARD.encode(
            crate::xchacha_encrypt_raw(
                &[8; 32],
                serde_json::json!({"sharedId":row.id,"virtualPath":"root"})
                    .to_string()
                    .as_bytes(),
            )
            .unwrap(),
        );
        let zip = s.archive(&row.id, &encrypted).unwrap();
        assert_eq!(
            zip.iter()
                .map(|e| e.virtual_path.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "empty/"]
        );
    }
    #[test]
    fn token_matches_standard_hmac_vector() {
        let (dir, s) = fixture();
        let root = dir.path().join("root");
        std::fs::create_dir(&root).unwrap();
        let mut row = create(&s, vec![root]);
        row.id = "Hi There".into();
        s.db.share_save(&row).unwrap();
        s.prefs
            .set("master_secret", STANDARD.encode([0x0b; 32]))
            .unwrap();
        let expected = "198a607eb44bfbc69903a0f1cf2bbdc5ba0aa3f3d9ae3c1c7a3b1696a0b68cf7";
        let actual = URL_SAFE.decode(s.token(&row.id).unwrap()).unwrap();
        // RFC 4231 uses a 20-byte key; this domain uses fixed 32-byte master secrets.
        assert_eq!(
            actual
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>(),
            expected
        );
    }
}
