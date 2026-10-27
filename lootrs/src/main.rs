use std::process::ExitCode;

use clap::Parser;
use lootrs::{app, cli::Cli, logger, settings::Settings};

fn main() -> ExitCode {
    let cli = Cli::parse();
    let settings = match Settings::resolve(cli.into(), |key| std::env::var_os(key)) {
        Ok(settings) => settings,
        Err(err) => {
            eprintln!("lootrs: {err}");
            return ExitCode::from(2);
        }
    };

    let _guard = match logger::init(&settings) {
        Ok(guard) => guard,
        Err(err) => {
            eprintln!("lootrs: failed to initialize logging: {err}");
            return ExitCode::FAILURE;
        }
    };

    match app::run(&settings) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            tracing::error!("{err}");
            // The stderr log layer is off while the TUI owns the terminal.
            if !settings.logging.stderr {
                eprintln!("lootrs: {err}");
            }
            ExitCode::from(err.exit_code())
        }
    }
}
