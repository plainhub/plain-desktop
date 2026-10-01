use std::fmt;

/// A browsing failure: a rejected identifier, an empty required input, or
/// the underlying SQLite error.
#[derive(Debug)]
pub enum SqliteBrowseError {
    Invalid(String),
    Sqlite(rusqlite::Error),
}

impl fmt::Display for SqliteBrowseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SqliteBrowseError::Invalid(msg) => write!(f, "{msg}"),
            SqliteBrowseError::Sqlite(e) => write!(f, "sqlite: {e}"),
        }
    }
}

impl std::error::Error for SqliteBrowseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SqliteBrowseError::Invalid(_) => None,
            SqliteBrowseError::Sqlite(e) => Some(e),
        }
    }
}

impl From<rusqlite::Error> for SqliteBrowseError {
    fn from(e: rusqlite::Error) -> Self {
        SqliteBrowseError::Sqlite(e)
    }
}

pub type Result<T> = std::result::Result<T, SqliteBrowseError>;

/// One column of a table, as reported by `PRAGMA table_info`.
pub struct TableColumnMeta {
    pub name: String,
    pub data_type: String,
    pub not_null: bool,
    pub default_value: Option<String>,
    pub primary_key: bool,
}

/// The standard SQLite column types, for mapping a declared type onto a
/// UI-facing enum; anything else (or undeclared) is `Unknown`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqliteColumnType {
    Text,
    Integer,
    Real,
    Blob,
    Numeric,
    Unknown,
}

/// Map a declared column type (as `PRAGMA table_info` reports it) onto
/// [`SqliteColumnType`].
pub fn column_type_of(declared: &str) -> SqliteColumnType {
    match declared.to_ascii_uppercase().as_str() {
        "TEXT" => SqliteColumnType::Text,
        "INTEGER" => SqliteColumnType::Integer,
        "REAL" => SqliteColumnType::Real,
        "BLOB" => SqliteColumnType::Blob,
        "NUMERIC" => SqliteColumnType::Numeric,
        _ => SqliteColumnType::Unknown,
    }
}
