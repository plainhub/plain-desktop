use anyhow::Result;
use plain_server::platform::system::consts::AppPaths;

pub fn run(paths: &AppPaths) -> Result<()> {
    plain_rs::platform::linux::install::service::require_root()?;
    let password = plain_rs::platform::unix::read_password::prompt_password("New password: ")?;
    let confirm = plain_rs::platform::unix::read_password::prompt_password("Confirm: ")?;
    anyhow::ensure!(password == confirm, "Passwords do not match");
    anyhow::ensure!(!password.is_empty(), "Password cannot be empty");
    plain_server::assembly::nas::set_password(&paths.data_dir, &password)?;
    println!("Admin password updated.");
    Ok(())
}
