use anyhow::Result;

pub fn run(with_libreoffice: bool) -> Result<()> {
    plain_rs::platform::linux::install::service::install(with_libreoffice)?;
    println!("plainapp install: done");
    Ok(())
}
