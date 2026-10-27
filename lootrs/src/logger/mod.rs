//! `tracing` subscriber setup: a daily-rotated file log, plus stderr for CLI commands.

use std::{fmt as std_fmt, io};

use tracing::{Event, Level, Subscriber};
use tracing_appender::{
    non_blocking::WorkerGuard,
    rolling::{InitError, RollingFileAppender, Rotation},
};
use tracing_subscriber::{
    filter::LevelFilter,
    fmt::{self, FmtContext, FormatEvent, FormatFields, format::Writer},
    layer::{Layer, SubscriberExt},
    registry::LookupSpan,
};

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
            .event_format(CliFormat)
            .with_filter(stderr_filter(logging.level))
    });

    let subscriber = tracing_subscriber::registry()
        .with(level_filter(logging.level))
        .with(file_layer)
        .with(stderr_layer);

    Ok((subscriber, guard))
}

/// The terminal gets warnings and errors; `info` stays in the file because
/// commands print their own results. `-v`/`-vv` show debug/trace there too.
pub fn stderr_filter(level: LogLevel) -> LevelFilter {
    match level {
        LogLevel::Info => LevelFilter::WARN,
        level => level_filter(level),
    }
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

/// Stderr format for CLI commands: `lootrs: warning: message`, no spans or timestamps.
pub struct CliFormat;

impl<S, N> FormatEvent<S, N> for CliFormat
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> std_fmt::Result {
        let level = match *event.metadata().level() {
            Level::ERROR => "error",
            Level::WARN => "warning",
            Level::INFO => "info",
            Level::DEBUG => "debug",
            Level::TRACE => "trace",
        };
        write!(writer, "lootrs: {level}: ")?;
        ctx.field_format().format_fields(writer.by_ref(), event)?;
        writeln!(writer)
    }
}
