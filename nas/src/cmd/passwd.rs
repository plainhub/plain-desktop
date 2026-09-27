//! `plain-nas passwd` - interactively reset the admin password.
use crate::consts::AppPaths;
use crate::crypto;
use crate::db;
use crate::read_password;
use anyhow::Result;

pub fn run(paths: &AppPaths) -> Result<()> {
    if !nix::unistd::Uid::current().is_root() {
        eprintln!("This command requires root privileges. Please run with sudo.");
        std::process::exit(1);
    }
    std::fs::create_dir_all(&paths.data_dir).ok();
    let prefs = crate::prefs::Prefs::load(&crate::prefs::default_path(&paths.data_dir))?;
    let pw = db::PasswordStore::new(&prefs);
    let password = read_password::prompt_password("New password: ")?;
    let confirm = read_password::prompt_password("Confirm: ")?;
    if password != confirm {
        eprintln!("Passwords do not match");
        std::process::exit(1);
    }
    if password.is_empty() {
        eprintln!("Password cannot be empty");
        std::process::exit(1);
    }
    let hash = crypto::sha512_hex(&password);
    pw.set(&hash)?;
    println!("Admin password updated.");
    Ok(())
}
