//! Host bootstrap for the media stack: one call that installs the
//! process-global media paths, opens the fjall store and hands the host
//! everything the media GraphQL roots and the thumbnail-serving `/fs`
//! branch need. plain-nas wires this at `cmd::run`; the desktop's local
//! server wires it at startup.

use std::path::Path;
use std::sync::Arc;

use crate::media::kv::Db;

pub struct MediaService {
    /// The fjall store backing media rows, trash, events and sessions.
    pub db: Arc<Db>,
}

impl MediaService {
    /// Install media paths (`data_dir` + `cache_dir`) and open the fjall
    /// store at `<data_dir>/fjall` — also installing it as the
    /// process-global kv default so every `kv::get_default()` caller
    /// (file tasks, trash, event logs, the nas automount watcher) shares
    /// this one handle. Also warms the thumbnail engine with default
    /// budgets; hosts that load a config file can additionally call
    /// [`crate::media::thumb::init_from_config`].
    pub fn init(data_dir: &Path, cache_dir: &Path) -> anyhow::Result<Self> {
        crate::media::paths::set(data_dir.to_path_buf(), cache_dir.to_path_buf());
        let db = Db::open(&data_dir.join("fjall"))?;
        let _ = crate::media::kv::set_default(db.clone());
        Ok(Self { db: Arc::new(db) })
    }
}
