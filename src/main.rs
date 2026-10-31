//! Halloween door kiosk: background music on shuffle, SPACE plays the door song.
//!
//! The process you start is a tiny supervisor. It runs the real player as a child
//! process and restarts it if it ever crashes or hangs.

mod app;
mod audio;
mod library;
mod ui;

use app::App;
use audio::{Rodio, TrackSource};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{
    env, fs,
    io::{self, IsTerminal},
    os::{fd::AsRawFd, unix::process::ExitStatusExt},
    path::{Path, PathBuf},
    process::{Command, ExitCode},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

const WORKER_ENV: &str = "HALLOWEENER_WORKER";
const EXIT_BAD_SETUP: u8 = 2;
/// Main loop stopped running for this long: give up and let the supervisor restart us.
const HANG_LIMIT_SECS: u64 = 15;
const FRAME_TIME: Duration = Duration::from_millis(50);
const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;

const USAGE: &str = "\
Usage: halloweener [--background-volume PERCENT] <background> <door-song>

  <background>    Folder of music played on shuffle forever,
                  or a single file looped forever
  <door-song>     The song to play when SPACE is pressed

Options:
  --background-volume PERCENT
                  Background volume relative to the door song, 0-100 (default: 33)

Keys:
  SPACE           Start the door song / go back to background music
  + or =          Volume up
  -               Volume down
  Ctrl+Q          Quit (the only way out; everything else is ignored)

Supported formats: mp3, flac, ogg, wav, m4a, mp4";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    if env::var_os(WORKER_ENV).is_some() {
        worker(&args)
    } else {
        supervise(&args)
    }
}

// ---------------------------------------------------------------- supervisor

fn supervise(args: &[String]) -> ExitCode {
    let stop = Arc::new(AtomicBool::new(false));
    register_stop_signals(&stop);
    loop {
        let spawn = |exe: PathBuf| Command::new(exe).args(args).env(WORKER_ENV, "1").spawn();
        // If the binary was rebuilt or removed since we started, its path is gone, but
        // the kernel still has the running one. Keeps crash restarts working regardless.
        let spawned = env::current_exe()
            .and_then(spawn)
            .or_else(|_| spawn(PathBuf::from("/proc/self/exe")));
        let mut child = match spawned {
            Ok(child) => child,
            Err(err) => {
                eprintln!("error: cannot start player: {err}");
                return ExitCode::from(EXIT_BAD_SETUP);
            }
        };
        let status = loop {
            if stop.load(Ordering::SeqCst) {
                terminate(&mut child);
                reset_terminal();
                return ExitCode::SUCCESS;
            }
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) => thread::sleep(Duration::from_millis(100)),
                Err(_) => break None,
            }
        };
        match status {
            Some(status) if status.success() => return ExitCode::SUCCESS,
            Some(status) if status.code() == Some(EXIT_BAD_SETUP.into()) => {
                return ExitCode::from(EXIT_BAD_SETUP);
            }
            Some(status) => {
                reset_terminal();
                eprintln!(
                    "player stopped unexpectedly (code {:?}, signal {:?}), restarting...",
                    status.code(),
                    status.signal()
                );
            }
            None => reset_terminal(),
        }
        thread::sleep(Duration::from_secs(1));
    }
}

fn terminate(child: &mut std::process::Child) {
    // SAFETY: plain syscall on a child we have not yet reaped.
    unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGTERM) };
    for _ in 0..20 {
        if matches!(child.try_wait(), Ok(Some(_))) {
            return;
        }
        thread::sleep(Duration::from_millis(100));
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn register_stop_signals(flag: &Arc<AtomicBool>) {
    for signal in [
        signal_hook::consts::SIGTERM,
        signal_hook::consts::SIGINT,
        signal_hook::consts::SIGHUP,
    ] {
        if let Err(err) = signal_hook::flag::register(signal, flag.clone()) {
            eprintln!("cannot watch signal {signal}: {err}");
        }
    }
}

fn reset_terminal() {
    let _ = terminal::disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen, cursor::Show);
}

// -------------------------------------------------------------------- worker

struct Setup {
    background: PathBuf,
    background_volume: f32,
    door_source: TrackSource,
    door_title: String,
    door_duration: Option<Duration>,
}

const DEFAULT_BACKGROUND_PERCENT: u8 = 33;

struct Options {
    background: PathBuf,
    door_song: PathBuf,
    /// Linear factor, 0.0 to 1.0.
    background_volume: f32,
}

fn parse_percent(text: &str) -> Result<f32, String> {
    match text.trim_end_matches('%').parse::<u8>() {
        Ok(percent) if percent <= 100 => Ok(f32::from(percent) / 100.0),
        _ => Err(format!("--background-volume must be a number from 0 to 100, not \"{text}\"")),
    }
}

fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut paths = Vec::new();
    let mut background_volume = f32::from(DEFAULT_BACKGROUND_PERCENT) / 100.0;
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        if arg == "--background-volume" {
            let value = rest.next().ok_or("--background-volume needs a number from 0 to 100")?;
            background_volume = parse_percent(value)?;
        } else if let Some(value) = arg.strip_prefix("--background-volume=") {
            background_volume = parse_percent(value)?;
        } else if arg.starts_with("--") {
            return Err(format!("unknown option {arg}\n\n{USAGE}"));
        } else {
            paths.push(PathBuf::from(arg));
        }
    }
    let [background, door_song] = <[PathBuf; 2]>::try_from(paths).map_err(|_| USAGE.to_string())?;
    Ok(Options { background, door_song, background_volume })
}

/// Catches mistakes while somebody is still looking at the command line.
fn check_setup(args: &[String]) -> Result<Setup, String> {
    let Options { background, door_song, background_volume } = parse_args(args)?;
    let door_song = door_song.as_path();

    let tracks = library::scan(&background)
        .map_err(|err| format!("cannot read background music {}: {err}", background.display()))?;
    if tracks.is_empty() {
        return Err(format!(
            "no music files in {} (looking for: {})",
            background.display(),
            library::EXTENSIONS.join(", ")
        ));
    }
    if background.is_file() {
        // The only song there is, so a broken one is worth stopping for.
        audio::probe(&TrackSource::File(background.clone()))
            .map_err(|err| format!("cannot decode background song {}: {err}", background.display()))?;
    }

    let bytes = fs::read(door_song)
        .map_err(|err| format!("cannot read door song {}: {err}", door_song.display()))?;
    let door_source = TrackSource::Memory {
        bytes: bytes.into(),
        extension: door_song.extension().map(|ext| ext.to_string_lossy().to_lowercase()),
    };
    let door_duration = audio::probe(&door_source)
        .map_err(|err| format!("cannot decode door song {}: {err}", door_song.display()))?;

    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err("must be started in a terminal".to_string());
    }
    Ok(Setup {
        background,
        background_volume,
        door_source,
        door_title: library::title_of(door_song),
        door_duration,
    })
}

fn worker(args: &[String]) -> ExitCode {
    let setup = match check_setup(args) {
        Ok(setup) => setup,
        Err(message) => {
            eprintln!("error: {message}");
            return ExitCode::from(EXIT_BAD_SETUP);
        }
    };
    redirect_stderr_to_log();
    let heartbeat = start_hang_watchdog();
    match run(setup, &heartbeat) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("terminal error: {err}");
            ExitCode::FAILURE
        }
    }
}

/// ALSA and friends print to stderr, which would scribble all over the screen.
fn redirect_stderr_to_log() {
    let path = log_path();
    if fs::metadata(&path).is_ok_and(|meta| meta.len() > MAX_LOG_BYTES) {
        let _ = fs::remove_file(&path);
    }
    let file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .or_else(|_| fs::OpenOptions::new().write(true).open("/dev/null"));
    if let Ok(file) = file {
        // SAFETY: both descriptors are valid; dup2 leaves its own copy open.
        unsafe { libc::dup2(file.as_raw_fd(), libc::STDERR_FILENO) };
    }
}

fn log_path() -> PathBuf {
    let dir = env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| Path::new(&home).join(".local/state")))
        .unwrap_or_else(env::temp_dir);
    let _ = fs::create_dir_all(&dir);
    dir.join("halloweener.log")
}

/// Exits if the main loop stops ticking, whatever the cause. Counts checks rather
/// than clock time, so a suspended laptop doesn't look like a hang.
fn start_hang_watchdog() -> Arc<AtomicU64> {
    let heartbeat = Arc::new(AtomicU64::new(0));
    let watched = heartbeat.clone();
    thread::spawn(move || {
        let mut last = 0;
        let mut silent_checks = 0;
        loop {
            thread::sleep(Duration::from_secs(1));
            let current = watched.load(Ordering::Relaxed);
            silent_checks = if current == last { silent_checks + 1 } else { 0 };
            last = current;
            if silent_checks >= HANG_LIMIT_SECS {
                eprintln!("main loop hung for {HANG_LIMIT_SECS}s, exiting");
                // SAFETY: skips destructors on purpose, as they might hang too.
                unsafe { libc::_exit(70) };
            }
        }
    });
    heartbeat
}

/// Restores the terminal however `run` ends, including by panic.
struct ScreenGuard;
impl Drop for ScreenGuard {
    fn drop(&mut self) {
        reset_terminal();
    }
}

fn run(setup: Setup, heartbeat: &AtomicU64) -> io::Result<()> {
    let stop = Arc::new(AtomicBool::new(false));
    register_stop_signals(&stop);

    terminal::enable_raw_mode()?;
    let _guard = ScreenGuard;
    execute!(io::stdout(), EnterAlternateScreen, cursor::Hide)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    terminal.clear()?;

    let mut app = App::new(
        Rodio::new(setup.background_volume),
        setup.background,
        setup.door_source,
        setup.door_title,
        setup.door_duration,
    );

    let started = Instant::now();
    while !stop.load(Ordering::SeqCst) {
        heartbeat.fetch_add(1, Ordering::Relaxed);
        let mut timeout = FRAME_TIME;
        while event::poll(timeout)? {
            timeout = Duration::ZERO;
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Release {
                    continue;
                }
                match key.code {
                    KeyCode::Char(' ') => {
                        app.on_space(Instant::now());
                    }
                    KeyCode::Char('+' | '=') => app.volume_up(),
                    KeyCode::Char('-') => app.volume_down(),
                    KeyCode::Char('q' | 'Q') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(());
                    }
                    _ => {}
                }
            }
        }
        app.tick(Instant::now());
        terminal.draw(|frame| ui::draw(frame, &app.view(), started.elapsed()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|arg| arg.to_string()).collect()
    }

    #[test]
    fn background_volume_defaults_to_a_third() {
        let options = parse_args(&args(&["music", "door.mp3"])).unwrap();
        assert_eq!(options.background_volume, 0.33);
        assert_eq!(options.background, PathBuf::from("music"));
        assert_eq!(options.door_song, PathBuf::from("door.mp3"));
    }

    #[test]
    fn background_volume_can_be_given_in_any_position_and_form() {
        for form in [
            &["--background-volume", "30", "music", "door.mp3"][..],
            &["music", "door.mp3", "--background-volume=30"][..],
            &["music", "--background-volume", "30%", "door.mp3"][..],
        ] {
            assert_eq!(parse_args(&args(form)).unwrap().background_volume, 0.3);
        }
    }

    #[test]
    fn bad_arguments_are_rejected() {
        for bad in [
            &["--background-volume", "101", "music", "door.mp3"][..],
            &["--background-volume", "-5", "music", "door.mp3"][..],
            &["--background-volume", "loud", "music", "door.mp3"][..],
            &["music", "door.mp3", "--background-volume"][..],
            &["--volume=3", "music", "door.mp3"][..],
            &["music"][..],
            &["music", "door.mp3", "extra"][..],
        ] {
            assert!(parse_args(&args(bad)).is_err(), "{bad:?}");
        }
    }
}
