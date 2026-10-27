use clap::{CommandFactory, Parser, error::ErrorKind};

use super::*;

fn parse(args: &[&str]) -> CliOverrides {
    Cli::try_parse_from(std::iter::once("lootrs").chain(args.iter().copied()))
        .unwrap()
        .into()
}

fn parse_err(args: &[&str]) -> ErrorKind {
    Cli::try_parse_from(std::iter::once("lootrs").chain(args.iter().copied()))
        .unwrap_err()
        .kind()
}

#[test]
fn cli_definition_is_valid() {
    Cli::command().debug_assert();
}

#[cfg(feature = "tui")]
#[test]
fn no_subcommand_opens_tui() {
    assert_eq!(parse(&[]).action, Action::Tui);
}

#[cfg(not(feature = "tui"))]
#[test]
fn no_subcommand_requires_one_without_tui() {
    assert_eq!(
        parse_err(&[]),
        ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
    );
    assert_eq!(parse_err(&["-v"]), ErrorKind::MissingSubcommand);
}

#[test]
fn run_with_extra_args() {
    let cli = parse(&["run", "wow", "--wait", "--", "-console", "--wait"]);

    assert_eq!(
        cli.action,
        Action::Run {
            game: "wow".into(),
            wait: true,
            debug: false,
            args: vec!["-console".into(), "--wait".into()],
        }
    );
}

#[test]
fn execute_target_and_args() {
    let cli = parse(&["execute", "-p", "default", "/tmp/setup.exe", "--", "/S"]);

    assert_eq!(
        cli.action,
        Action::Execute {
            profile: "default".into(),
            target: "/tmp/setup.exe".into(),
            wait: false,
            debug: false,
            args: vec!["/S".into()],
        }
    );
}

#[test]
fn profile_is_required() {
    assert_eq!(parse_err(&["init"]), ErrorKind::MissingRequiredArgument);
    assert_eq!(parse_err(&["tricks"]), ErrorKind::MissingRequiredArgument);
}

#[test]
fn tricks_verbs() {
    let cli = parse(&["tricks", "--profile", "p", "--", "vcrun2022", "corefonts"]);

    assert_eq!(
        cli.action,
        Action::Tricks {
            profile: "p".into(),
            verbs: vec!["vcrun2022".into(), "corefonts".into()],
        }
    );
}

#[test]
fn check_targets() {
    assert_eq!(
        parse(&["check"]).action,
        Action::Check {
            target: CheckTarget::All
        }
    );
    assert_eq!(
        parse(&["check", "wow"]).action,
        Action::Check {
            target: CheckTarget::Game("wow".into())
        }
    );
    assert_eq!(
        parse(&["check", "-p", "default"]).action,
        Action::Check {
            target: CheckTarget::Profile("default".into())
        }
    );
    assert_eq!(
        parse_err(&["check", "wow", "-p", "default"]),
        ErrorKind::ArgumentConflict
    );
}

#[test]
fn verbosity_maps_to_log_level() {
    assert_eq!(parse(&["list"]).log_level, None);
    assert_eq!(parse(&["list", "-v"]).log_level, Some(LogLevel::Debug));
    assert_eq!(parse(&["list", "-vv"]).log_level, Some(LogLevel::Trace));
    assert_eq!(parse(&["list", "-q"]).log_level, Some(LogLevel::Error));
    assert_eq!(
        parse(&["list", "--log-level", "warn"]).log_level,
        Some(LogLevel::Warn)
    );
}

#[test]
fn verbosity_flags_conflict() {
    assert_eq!(
        parse_err(&["list", "-v", "-q"]),
        ErrorKind::ArgumentConflict
    );
    assert_eq!(
        parse_err(&["list", "-v", "--log-level", "info"]),
        ErrorKind::ArgumentConflict
    );
}

#[test]
fn global_paths_after_subcommand() {
    let cli = parse(&["status", "--config-dir", "/c", "--log-dir", "/l", "--json"]);

    assert_eq!(cli.config_dir.as_deref(), Some(std::path::Path::new("/c")));
    assert_eq!(cli.log_dir.as_deref(), Some(std::path::Path::new("/l")));
    assert_eq!(cli.action, Action::Status { json: true });
}

#[test]
fn logs_path_flag() {
    assert_eq!(
        parse(&["logs", "wow", "--path"]).action,
        Action::Logs {
            game: "wow".into(),
            path_only: true,
        }
    );
}
