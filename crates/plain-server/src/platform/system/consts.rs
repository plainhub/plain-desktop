use std::path::PathBuf;

#[derive(Clone)]
pub struct AppPaths {
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub tls_cert: PathBuf,
    pub tls_key: PathBuf,
}

impl AppPaths {
    #[doc(hidden)]
    pub fn pin_test_data_dir() -> PathBuf {
        static PINNED: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
        PINNED
            .get_or_init(|| {
                let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-data-dir");
                std::fs::create_dir_all(&dir).expect("mkdir target/test-data-dir");
                unsafe { std::env::set_var("PLAIN_NAS_DATA_DIR", &dir) };
                dir
            })
            .clone()
    }

    pub fn detect() -> Self {
        Self::detect_with_data_directory(None)
    }

    pub fn detect_with_data_directory(directory: Option<PathBuf>) -> Self {
        let base = home_directory()
            .unwrap_or_else(std::env::temp_dir)
            .join(".plainapp");
        let data_dir = directory
            .or_else(|| configured_path("PLAINAPP_DATA_DIR", "PLAIN_NAS_DATA_DIR"))
            .unwrap_or_else(|| base.join("data"));
        Self {
            cache_dir: configured_path("PLAINAPP_CACHE_DIR", "PLAIN_NAS_CACHE_DIR")
                .unwrap_or_else(|| base.join("cache")),
            tls_cert: configured_path("PLAINAPP_TLS_CERT", "PLAIN_NAS_TLS_CERT")
                .unwrap_or_else(|| data_dir.join("tls.pem")),
            tls_key: configured_path("PLAINAPP_TLS_KEY", "PLAIN_NAS_TLS_KEY")
                .unwrap_or_else(|| data_dir.join("tls.key")),
            data_dir,
        }
    }
}

fn configured_path(name: &str, previous: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .or_else(|| std::env::var_os(previous))
        .map(PathBuf::from)
}

pub fn home_directory() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
}
