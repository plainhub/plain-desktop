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

#[test]
fn general_run_accepts_multiple_media_directories_and_global_paths() {
    let cli = Cli::try_parse_from([
        "plainapp",
        "run",
        "--data-dir",
        "data",
        "--cache-dir",
        "cache",
        "--media-dir",
        "photos",
        "--media-dir",
        "documents",
        "--web-root",
        "web",
    ])
    .unwrap();
    let paths = cli.paths().unwrap();
    assert!(paths.data_dir.is_absolute());
    assert!(paths.data_dir.ends_with("data"));
    assert!(paths.cache_dir.ends_with("cache"));
    assert!(paths.tls_cert.ends_with("data/tls.pem"));
    assert!(paths.tls_key.ends_with("data/tls.key"));
    match cli.command {
        Cmd::Run {
            media_directories,
            web_root,
            automount,
        } => {
            assert_eq!(
                media_directories,
                [PathBuf::from("photos"), PathBuf::from("documents")]
            );
            assert_eq!(web_root, Some(PathBuf::from("web")));
            assert!(!automount);
        }
        _ => panic!("run command required"),
    }
}

#[test]
fn password_input_and_linux_automount_are_explicit_options() {
    assert!(matches!(
        Cli::try_parse_from(["plainapp", "passwd", "--stdin"])
            .unwrap()
            .command,
        Cmd::Passwd { stdin: true }
    ));
    assert!(matches!(
        Cli::try_parse_from(["plainapp", "run", "--automount"])
            .unwrap()
            .command,
        Cmd::Run {
            automount: true,
            ..
        }
    ));
}
