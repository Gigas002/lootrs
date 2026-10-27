//! `tracing` subscriber setup: a daily-rotated file log, plus stderr for CLI commands.

use std::io;

use tracing::Subscriber;
use tracing_appender::{
    non_blocking::WorkerGuard,
    rolling::{InitError, RollingFileAppender, Rotation},
};
use tracing_subscriber::{filter::LevelFilter, fmt, layer::SubscriberExt};

use crate::{config::LogLevel, settings::Settings};

#[cfg(test)]
mod tests;

/// App log files are `lootrs.<date>.log` in the log directory.
pub const FILE_PREFIX: &str = "lootrs";
pub const FILE_SUFFIX: &str = "log";

#[derive(Debug, thiserror::Error)]
pub enum LoggerError {
    #[error("failed to create log directory: {0}")]
    CreateDir(#[source] io::Error),

    #[error(transparent)]
    Appender(#[from] InitError),

    #[error(transparent)]
    SetGlobal(#[from] tracing::subscriber::SetGlobalDefaultError),
}

/// Installs the global subscriber. Keep the guard alive until exit so
/// buffered log lines are flushed.
pub fn init(settings: &Settings) -> Result<WorkerGuard, LoggerError> {
    let (subscriber, guard) = subscriber(settings)?;
    tracing::subscriber::set_global_default(subscriber)?;
    Ok(guard)
}

/// Builds the subscriber without installing it.
pub fn subscriber(
    settings: &Settings,
) -> Result<(impl Subscriber + Send + Sync + use<>, WorkerGuard), LoggerError> {
    let logging = &settings.logging;
    std::fs::create_dir_all(&logging.dir).map_err(LoggerError::CreateDir)?;

    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix(FILE_PREFIX)
        .filename_suffix(FILE_SUFFIX)
        .max_log_files(logging.keep)
        .build(&logging.dir)?;
    let (writer, guard) = tracing_appender::non_blocking(appender);

    let file_layer = fmt::layer().with_writer(writer).with_ansi(false);
    let stderr_layer = logging.stderr.then(|| {
        fmt::layer()
            .with_writer(io::stderr)
            .with_ansi(false)
            .without_time()
            .with_target(false)
    });

    let subscriber = tracing_subscriber::registry()
        .with(level_filter(logging.level))
        .with(file_layer)
        .with(stderr_layer);

    Ok((subscriber, guard))
}

pub fn level_filter(level: LogLevel) -> LevelFilter {
    match level {
        LogLevel::Error => LevelFilter::ERROR,
        LogLevel::Warn => LevelFilter::WARN,
        LogLevel::Info => LevelFilter::INFO,
        LogLevel::Debug => LevelFilter::DEBUG,
        LogLevel::Trace => LevelFilter::TRACE,
    }
}
