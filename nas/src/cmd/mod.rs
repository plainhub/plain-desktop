//! Subcommand implementations. Mirrors the `cmd/*.go` files from the Go
//! project. Each module exposes a single `run` function.

pub mod bench;
pub mod install;
pub mod passwd;
pub mod run;
pub mod uninstall;
pub mod update;
