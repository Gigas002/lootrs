//! Launching and supervising a game or one-off executable.
//!
//! The supervisor owns the run log: it writes the header, timestamps every line
//! of the runner's output, waits for exit and writes the footer. `run` without
//! `--wait` re-executes lootrs as a detached supervisor (new session, hidden
//! `--supervised` flag), which reports one status line back over its stdout:
//! `ok\t<pid>\t<log>` or `error\t<exit code>` followed by the message.

use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::File,
    io::{self, BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

use chrono::{DateTime, Local};
use liblootrs::{
    diag::{self, SystemInfo},
    launch::{self, LaunchContext, LaunchSpec, Tag, Target},
    model::{Issue, Profile, Runner, Severity},
    prefix::{self, EnsureContext, PrefixError},
    process::{self, Owner},
    runlog::{self, RunKind, RunLog},
    store::Catalog,
};

use super::{AppError, validation_context};
use crate::settings::{Action, LaunchMode, Settings};

#[cfg(test)]
mod tests;

/// How long to keep reading output after the main process exits; children
/// such as `wineserver` may hold the pipes open much longer.
const DRAIN_TIMEOUT: Duration = Duration::from_secs(2);

/// What to launch, independent of how it was requested.
#[derive(Debug, Clone, Copy)]
pub enum Request<'a> {
    Game {
        name: &'a str,
    },
    Execute {
        profile: &'a str,
        program: &'a Path,
    },
    Tricks {
        profile: &'a str,
        verbs: &'a [String],
    },
}

/// A started process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Started {
    pub pid: u32,
    pub log: PathBuf,
}

/// Entry point for `run` and `execute`; returns the process exit code for lootrs.
pub fn launch(
    settings: &Settings,
    request: Request,
    mode: LaunchMode,
    debug: bool,
    args: &[String],
    out: &mut impl Write,
) -> Result<u8, AppError> {
    match mode {
        LaunchMode::Detach => {
            let started = spawn_detached(settings)?;
            writeln!(
                out,
                "started {} (pid {})\nlog: {}",
                describe(request),
                started.pid,
                started.log.display()
            )?;
            Ok(0)
        }
        LaunchMode::Wait => supervise(settings, request, debug, args, true, |_| {}),
        LaunchMode::Supervised => {
            // Leave the terminal's session so closing it doesn't kill the game.
            if let Err(err) = rustix::process::setsid() {
                tracing::warn!("setsid failed: {err}");
            }
            let mut reported = false;
            let result = supervise(settings, request, debug, args, false, |started| {
                // The parent stops reading after this line.
                let mut status = io::stdout();
                let _ = writeln!(status, "ok\t{}\t{}", started.pid, started.log.display());
                let _ = status.flush();
                reported = true;
            });
            match result {
                // Before "ok" the parent is still listening and reports the error itself.
                Err(err) if !reported => {
                    let _ = writeln!(io::stdout(), "error\t{}\n{err}", err.exit_code());
                    Ok(0)
                }
                // Nobody sees the detached supervisor's exit code.
                result => result.map(|_| 0),
            }
        }
    }
}

fn describe(request: Request) -> String {
    match request {
        Request::Game { name } => format!("`{name}`"),
        Request::Execute { profile, program } => {
            format!("`{}` with profile `{profile}`", program.display())
        }
        Request::Tricks { profile, .. } => format!("winetricks for profile `{profile}`"),
    }
}

/// Re-executes lootrs as a detached supervisor and waits for its status line.
fn spawn_detached(settings: &Settings) -> Result<Started, AppError> {
    let exe = std::env::current_exe().map_err(AppError::Detach)?;
    let mut child = Command::new(exe)
        .args(supervised_args(settings))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(AppError::Detach)?;

    let stdout = child.stdout.take().expect("stdout is piped");
    let status = read_status(stdout);
    // Reap the supervisor if it already exited (error case); otherwise it is
    // reparented to init when we exit.
    let _ = child.try_wait();
    status
}

/// Parses the supervisor's status report.
pub fn read_status(stdout: impl Read) -> Result<Started, AppError> {
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader.read_line(&mut line).map_err(AppError::Detach)?;

    let mut fields = line.trim_end_matches('\n').split('\t');
    match (fields.next(), fields.next(), fields.next()) {
        (Some("ok"), Some(pid), Some(log)) => Ok(Started {
            pid: pid.parse().unwrap_or_default(),
            log: log.into(),
        }),
        (Some("error"), Some(code), None) => {
            let mut message = String::new();
            reader
                .read_to_string(&mut message)
                .map_err(AppError::Detach)?;
            Err(AppError::Supervisor {
                code: code.parse().unwrap_or(1),
                message: message.trim_end().to_owned(),
            })
        }
        _ => Err(AppError::Supervisor {
            code: 1,
            message: "supervisor exited without reporting; see the app log".into(),
        }),
    }
}

/// CLI arguments that reproduce this invocation as a supervisor.
pub fn supervised_args(settings: &Settings) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec![
        "--config-dir".into(),
        settings.dirs.config.clone().into(),
        "--log-dir".into(),
        settings.logging.dir.clone().into(),
        "--log-level".into(),
        settings.logging.level.as_str().into(),
    ];
    let (debug, extra) = match &settings.action {
        Action::Run {
            game,
            debug,
            args: extra,
            ..
        } => {
            args.extend(["run".into(), game.into()]);
            (*debug, extra)
        }
        Action::Execute {
            profile,
            target,
            debug,
            args: extra,
            ..
        } => {
            args.extend([
                "execute".into(),
                "--profile".into(),
                profile.into(),
                target.clone().into(),
            ]);
            (*debug, extra)
        }
        action => unreachable!("`{}` is not launched through a supervisor", action.name()),
    };
    args.push("--supervised".into());
    if debug {
        args.push("--debug".into());
    }
    if !extra.is_empty() {
        args.push("--".into());
        args.extend(extra.iter().map(OsString::from));
    }
    args
}

/// Validates, prepares, spawns and waits; everything goes into the run log.
fn supervise(
    settings: &Settings,
    request: Request,
    debug: bool,
    extra_args: &[String],
    tee: bool,
    on_started: impl FnOnce(&Started),
) -> Result<u8, AppError> {
    let catalog = Catalog::load(&settings.dirs.config);
    let ctx = validation_context(settings);

    let (target, profile, issues, kind) = match request {
        Request::Game { name } => {
            let game = catalog
                .game(name)
                .ok_or_else(|| AppError::UnknownGame(name.to_owned()))?;
            let issues = catalog.validate_game(name, &ctx).unwrap_or_default();
            let profile = catalog.profile(&game.profile);
            (Target::Game(game), profile, issues, RunKind::Game(name))
        }
        Request::Execute { profile, program } => {
            let found = catalog
                .profile(profile)
                .ok_or_else(|| AppError::UnknownProfile(profile.to_owned()))?;
            (
                Target::Execute { program },
                Some(found),
                found.validate(&ctx),
                RunKind::Execute(profile),
            )
        }
        Request::Tricks { profile, verbs } => {
            let found = catalog
                .profile(profile)
                .ok_or_else(|| AppError::UnknownProfile(profile.to_owned()))?;
            (
                Target::Tricks { verbs },
                Some(found),
                found.validate(&ctx),
                RunKind::Tricks(profile),
            )
        }
    };
    check_issues(&issues)?;
    let profile = profile.expect("validation reports a missing profile");
    if let Request::Game { name } = request
        && process::is_running(&process::scan(), &Owner::Game(name.to_owned()))
    {
        return Err(AppError::AlreadyRunning(name.to_owned()));
    }

    let started_at = Local::now();
    let stamp = started_at.format("%Y%m%d-%H%M%S").to_string();
    let RunLog {
        file,
        path: log_path,
        artifacts,
    } = runlog::create(&settings.logging.dir, kind, &stamp, settings.logging.keep)
        .map_err(AppError::RunLog)?;
    let log = Arc::new(Mutex::new(file));

    let tag = match request {
        Request::Game { name } => Tag::Game(name.to_owned()),
        Request::Execute { profile, .. } => Tag::Session(format!("execute:{profile}:{stamp}")),
        Request::Tricks { profile, .. } => Tag::Session(format!("tricks:{profile}:{stamp}")),
    };

    let prefix = match ensure_prefix(settings, profile, &tag) {
        Ok(report) => report,
        Err(err) => {
            let _ = write_log(&log, &format!("=== prefix setup failed ===\n{err}\n"));
            runlog::remove_if_empty(&artifacts);
            return Err(AppError::Prefix(err));
        }
    };

    let prepared = launch::prepare(
        target,
        profile,
        &tag,
        &LaunchContext {
            home: &settings.dirs.home,
            search_path: &settings.search_path,
            debug,
            extra_args,
            artifacts_dir: &artifacts,
            battleye_runtime: settings.anticheat.battleye.as_deref(),
            eac_runtime: settings.anticheat.eac.as_deref(),
            umu_runtime_update: settings.umu_runtime_update,
        },
    );

    let header = header(
        request,
        target,
        profile,
        prepared.as_ref().ok(),
        prefix.as_ref(),
        started_at,
    );
    write_log(&log, &header).map_err(AppError::RunLog)?;

    let spec = match prepared {
        Ok(spec) => spec,
        Err(err) => {
            write_log(&log, &format!("=== preflight failed ===\n{err}\n"))
                .map_err(AppError::RunLog)?;
            runlog::remove_if_empty(&artifacts);
            return Err(AppError::Preflight(err));
        }
    };

    let mut child = match spawn(&spec) {
        Ok(child) => child,
        Err(source) => {
            let _ = write_log(&log, &format!("=== spawn error: {source} ===\n"));
            runlog::remove_if_empty(&artifacts);
            return Err(AppError::Spawn {
                program: spec.program,
                source,
            });
        }
    };
    let started = Started {
        pid: child.id(),
        log: log_path,
    };
    tracing::info!(pid = started.pid, log = %started.log.display(), "started {}", describe(request));
    on_started(&started);

    let drained = pump_output(&mut child, &log, tee);
    let status = child.wait().map_err(AppError::RunLog)?;
    let deadline = Instant::now() + DRAIN_TIMEOUT;
    for _ in 0..2 {
        let left = deadline.saturating_duration_since(Instant::now());
        if drained.recv_timeout(left).is_err() {
            break;
        }
    }

    let elapsed = Local::now() - started_at;
    let footer = footer(status, elapsed.to_std().unwrap_or_default());
    tracing::info!(
        "{} {}",
        describe(request),
        footer.trim_matches(['=', ' ', '\n'])
    );
    write_log(&log, &footer).map_err(AppError::RunLog)?;
    runlog::remove_if_empty(&artifacts);

    Ok(exit_code(status))
}

/// Logs warnings; fails on errors.
fn check_issues(issues: &[Issue]) -> Result<(), AppError> {
    let mut errors = Vec::new();
    for issue in issues {
        match issue.severity {
            Severity::Warning => tracing::warn!("{issue}"),
            Severity::Error => errors.push(issue.to_string()),
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(AppError::Invalid(errors.join("\n")))
    }
}

fn spawn(spec: &LaunchSpec) -> io::Result<Child> {
    Command::new(&spec.program)
        .args(&spec.args)
        .envs(&spec.env)
        .current_dir(&spec.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
}

/// Copies stdout/stderr line by line into the log; each reader signals on `rx` at EOF.
fn pump_output(child: &mut Child, log: &Arc<Mutex<File>>, tee: bool) -> mpsc::Receiver<()> {
    let (tx, rx) = mpsc::channel();
    let stdout = child.stdout.take().expect("stdout is piped");
    let stderr = child.stderr.take().expect("stderr is piped");

    for (stream, reader, label) in [
        (
            Stream::Stdout,
            Box::new(stdout) as Box<dyn Read + Send>,
            "stdout",
        ),
        (Stream::Stderr, Box::new(stderr), "stderr"),
    ] {
        let log = Arc::clone(log);
        let tx = tx.clone();
        thread::spawn(move || {
            let mut reader = BufReader::new(reader);
            let mut line = Vec::new();
            while reader.read_until(b'\n', &mut line).unwrap_or(0) > 0 {
                let text = String::from_utf8_lossy(&line);
                let text = text.trim_end_matches(['\n', '\r']);
                let stamp = Local::now().format("%H:%M:%S%.3f");
                let _ = write_log(&log, &format!("[{stamp}] [{label}] {text}\n"));
                if tee {
                    stream.echo(text);
                }
                line.clear();
            }
            let _ = tx.send(());
        });
    }
    rx
}

#[derive(Debug, Clone, Copy)]
enum Stream {
    Stdout,
    Stderr,
}

impl Stream {
    fn echo(self, line: &str) {
        let _ = match self {
            Self::Stdout => writeln!(io::stdout(), "{line}"),
            Self::Stderr => writeln!(io::stderr(), "{line}"),
        };
    }
}

fn write_log(log: &Mutex<File>, text: &str) -> io::Result<()> {
    let mut file = log.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    file.write_all(text.as_bytes())
}

/// Creates/updates the profile's prefix, with setup processes tagged like the run.
pub fn ensure_prefix(
    settings: &Settings,
    profile: &Profile,
    tag: &Tag,
) -> Result<Option<prefix::Report>, PrefixError> {
    let tags = BTreeMap::from([tag.env()]);
    prefix::ensure(
        profile,
        &EnsureContext {
            home: &settings.dirs.home,
            search_path: &settings.search_path,
            lock_dir: &settings.dirs.runtime,
            tags: &tags,
            umu_runtime_update: settings.umu_runtime_update,
        },
        |prefix| process::prefix_in_use(&process::scan(), prefix),
    )
}

/// `--- prefix ---` section for run logs: what setup did and, for wine, what is linked.
pub fn prefix_section(runner: Runner, report: &prefix::Report) -> String {
    let mut out = String::from("--- prefix ---\n");
    if report.created {
        out += "created\n";
    }
    for change in &report.changes {
        out += &format!("{change}\n");
    }
    if runner == Runner::Wine {
        for (name, linked) in [("dxvk", &report.state.dxvk), ("vkd3d", &report.state.vkd3d)] {
            let source = linked.as_ref().map_or("wine builtin".into(), |linked| {
                linked.source.display().to_string()
            });
            out += &format!("{name}: {source}\n");
        }
    }
    if !report.output.trim().is_empty() {
        let command = match runner {
            Runner::Umu => "createprefix",
            _ => "wineboot",
        };
        out += &format!("{command} output:\n{}\n", report.output.trim_end());
    }
    if out == "--- prefix ---\n" {
        out += "unchanged\n";
    }
    out
}

fn title(request: Request) -> String {
    match request {
        Request::Game { name } => format!("run {name}"),
        Request::Execute { program, .. } => format!("execute {}", program.display()),
        Request::Tricks { verbs: [], .. } => "tricks (GUI)".into(),
        Request::Tricks { verbs, .. } => format!("tricks {}", verbs.join(" ")),
    }
}

fn first_line(title: &str, started_at: DateTime<Local>) -> String {
    format!(
        "=== lootrs {} — {title} — {} ===\n",
        env!("CARGO_PKG_VERSION"),
        started_at.to_rfc3339_opts(chrono::SecondsFormat::Secs, false),
    )
}

/// Run-log header: what was launched, how, and on what system.
pub fn header(
    request: Request,
    target: Target,
    profile: &Profile,
    spec: Option<&LaunchSpec>,
    prefix: Option<&prefix::Report>,
    started_at: DateTime<Local>,
) -> String {
    let mut out = first_line(&title(request), started_at);

    if let Target::Game(game) = target {
        out += &format!(
            "--- game ---\n{}",
            toml::to_string(game).unwrap_or_default()
        );
    }
    out += &format!(
        "--- profile ---\n{}",
        toml::to_string(profile).unwrap_or_default()
    );

    if let Some(spec) = spec {
        out += &format!("--- command ---\n{}\n", diag::command_line(spec.argv()));
        out += &format!("cwd: {}\n", spec.cwd.display());
        out += &format!(
            "--- env (on top of inherited) ---\n{}",
            diag::env_lines(&spec.env)
        );
        if let Some(runner) = &spec.runner {
            let version = diag::runner_version(runner).unwrap_or_else(|| "unknown".into());
            out += &format!("runner: {version} ({})\n", runner.display());
        }
        if profile.runner == Runner::Umu
            && let Some(proton) = spec.env.get("PROTONPATH")
        {
            let proton = Path::new(proton);
            let version = diag::proton_version(proton).unwrap_or_else(|| "unknown".into());
            out += &format!("proton: {version} ({})\n", proton.display());
        }
    }

    if let Some(report) = prefix {
        out += &prefix_section(profile.runner, report);
    }

    let system = SystemInfo::collect();
    out += &format!(
        "--- system ---\nkernel: {}\ndistro: {}\nntsync: {}\nvulkan icds: {}\n",
        system.kernel.as_deref().unwrap_or("unknown"),
        system.distro.as_deref().unwrap_or("unknown"),
        if system.ntsync { "yes" } else { "no" },
        if system.vulkan_icds.is_empty() {
            "none".to_owned()
        } else {
            system.vulkan_icds.join(", ")
        },
    );
    out += "=== output ===\n";
    out
}

pub fn footer(status: ExitStatus, elapsed: Duration) -> String {
    use std::os::unix::process::ExitStatusExt;

    let secs = elapsed.as_secs();
    let duration = format!("{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60);
    let exit = match (status.code(), status.signal()) {
        (Some(code), _) => format!("code {code}"),
        (None, Some(signal)) => format!("signal {signal}"),
        (None, None) => "unknown".into(),
    };
    format!("=== exit: {exit} — duration {duration} ===\n")
}

/// Shell convention: the exit code, or 128 + signal.
pub fn exit_code(status: ExitStatus) -> u8 {
    use std::os::unix::process::ExitStatusExt;

    match (status.code(), status.signal()) {
        (Some(code), _) => u8::try_from(code).unwrap_or(1),
        (None, Some(signal)) => u8::try_from(128 + signal).unwrap_or(255),
        (None, None) => 1,
    }
}

/// `lootrs init`: creates or updates a profile's prefix, logged under `init/<profile>/`.
pub fn init(settings: &Settings, name: &str, out: &mut impl Write) -> Result<(), AppError> {
    let catalog = Catalog::load(&settings.dirs.config);
    let profile = catalog
        .profile(name)
        .ok_or_else(|| AppError::UnknownProfile(name.to_owned()))?;
    check_issues(&profile.validate(&validation_context(settings)))?;
    if profile.runner == Runner::Native {
        return Err(AppError::Invalid(format!(
            "profile `{name}` uses runner `native`, which has no prefix"
        )));
    }

    let started_at = Local::now();
    let stamp = started_at.format("%Y%m%d-%H%M%S").to_string();
    let RunLog {
        mut file,
        artifacts,
        ..
    } = runlog::create(
        &settings.logging.dir,
        RunKind::Init(name),
        &stamp,
        settings.logging.keep,
    )
    .map_err(AppError::RunLog)?;
    runlog::remove_if_empty(&artifacts);

    let mut log = first_line(&format!("init {name}"), started_at);
    log += &format!(
        "--- profile ---\n{}",
        toml::to_string(profile).unwrap_or_default()
    );
    let result = ensure_prefix(
        settings,
        profile,
        &Tag::Session(format!("init:{name}:{stamp}")),
    );
    match &result {
        Ok(Some(report)) => log += &prefix_section(profile.runner, report),
        Ok(None) => {}
        Err(err) => log += &format!("=== prefix setup failed ===\n{err}\n"),
    }
    file.write_all(log.as_bytes()).map_err(AppError::RunLog)?;

    let report = result.map_err(AppError::Prefix)?.unwrap_or_default();
    let prefix_path = profile
        .prefix
        .as_deref()
        .map(|p| liblootrs::paths::expand_tilde(p, &settings.dirs.home))
        .unwrap_or_default();
    let summary = match (report.created, report.changes.is_empty()) {
        (true, _) => "created",
        (false, false) => "updated",
        (false, true) => "up to date",
    };
    writeln!(out, "prefix {}: {summary}", prefix_path.display())?;
    for change in &report.changes {
        writeln!(out, "  {change}")?;
    }
    tracing::info!(profile = name, summary, "init");
    Ok(())
}
