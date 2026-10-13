#[cfg(feature = "api")]
pub mod auth;
#[cfg(feature = "api")]
pub mod chunked_upload;
#[cfg(feature = "api")]
pub mod cors;
#[cfg(feature = "api")]
pub mod file_server;
#[cfg(feature = "api")]
pub mod handlers;
#[cfg(feature = "api")]
pub mod media_alias;
#[cfg(feature = "api")]
pub mod proxy_file;
#[cfg(feature = "api")]
pub mod request_key;
#[cfg(feature = "api")]
pub mod response;
#[cfg(feature = "api")]
pub mod static_files;
#[cfg(feature = "api")]
pub mod upload;
#[cfg(feature = "api")]
pub mod zip;

#[cfg(test)]
#[path = "../../../../tests/unit/http/routes/local_shell_auth.rs"]
mod shell_auth_tests;
