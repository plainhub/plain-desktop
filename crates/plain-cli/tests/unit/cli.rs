use super::*;
use clap::CommandFactory;

#[test]
fn renamed_cli_keeps_all_existing_command_entries() {
    let command = Cli::command();
    assert_eq!(command.get_name(), "plainapp");
    for name in ["install", "uninstall", "passwd", "update", "bench", "run"] {
        assert!(command.find_subcommand(name).is_some(), "{name}");
        assert!(Cli::try_parse_from(["plainapp", name]).is_ok(), "{name}");
    }
}

#[test]
fn installation_and_uninstallation_options_are_preserved() {
    assert!(matches!(
        Cli::try_parse_from(["plainapp", "install", "--with-libreoffice"])
            .unwrap()
            .command,
        Cmd::Install {
            with_libreoffice: true
        }
    ));
    assert!(matches!(
        Cli::try_parse_from(["plainapp", "uninstall", "--keep-config", "--keep-data"])
            .unwrap()
            .command,
        Cmd::Uninstall {
            keep_config: true,
            keep_data: true
        }
    ));
}
