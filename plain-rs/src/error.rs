/// Library-domain error: a SQLite failure or a domain-level message
/// (e.g. "tag not found"). `Send + Sync` so it converts into anyhow and
/// GraphQL error types at the consumers' boundaries.
#[derive(Debug)]
pub enum DomainError {
    Sqlite(rusqlite::Error),
    Other(String),
}

impl std::fmt::Display for DomainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DomainError::Sqlite(e) => write!(f, "sqlite: {e}"),
            DomainError::Other(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for DomainError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            DomainError::Sqlite(e) => Some(e),
            DomainError::Other(_) => None,
        }
    }
}

impl From<rusqlite::Error> for DomainError {
    fn from(e: rusqlite::Error) -> Self {
        DomainError::Sqlite(e)
    }
}

pub type DomainResult<T> = std::result::Result<T, DomainError>;
