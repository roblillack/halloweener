//! Playback logic: what should be playing, independent of screen and sound card.

use crate::{
    audio::{Backend, Slot, TrackSource},
    library::{title_of, ShuffleBag},
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

/// Ignore SPACE until the key has been quiet this long. Holding the key, or an
/// excited double tap, must not flip the mode. It has to outlast the keyboard's
/// initial auto-repeat delay (typically 250-660 ms).
pub const SPACE_QUIET_TIME: Duration = Duration::from_millis(800);
/// A playing track whose position stands still this long means dead audio output.
const STALL_TIMEOUT: Duration = Duration::from_secs(4);
const DEVICE_RETRY: Duration = Duration::from_secs(2);
const NO_MUSIC_RETRY: Duration = Duration::from_secs(5);
/// A track ending sooner than this counts as a failure, which keeps a folder of
/// broken files from spinning.
const MIN_HEALTHY_PLAY: Duration = Duration::from_secs(1);
const VOLUME_STEP: u8 = 5;

/// Squared, because hearing is logarithmic: this makes each step at the volume
/// keys sound like a similar change, where a linear scale barely moves near the top.
fn master_gain(percent: u8) -> f32 {
    let fraction = f32::from(percent) / 100.0;
    fraction * fraction
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Background,
    Door,
}

struct Track {
    source: TrackSource,
    title: String,
    duration: Option<Duration>,
    /// Last known position; where playback resumes after losing the device.
    position: Duration,
    /// Whether the backend currently holds this track.
    loaded: bool,
}

impl Track {
    fn new(source: TrackSource, title: String, duration: Option<Duration>) -> Self {
        Self { source, title, duration, position: Duration::ZERO, loaded: false }
    }
}

pub struct View<'a> {
    pub mode: Mode,
    pub title: Option<&'a str>,
    pub position: Duration,
    pub duration: Option<Duration>,
    pub warning: Option<&'static str>,
    pub volume: u8,
}

pub struct App<B: Backend> {
    backend: B,
    mode: Mode,
    background: Option<Track>,
    door: Track,
    bag: ShuffleBag,
    failures: usize,
    no_music: bool,
    next_track_at: Option<Instant>,
    next_open_at: Option<Instant>,
    last_space: Option<Instant>,
    stall: Option<(Duration, Instant)>,
    volume: u8,
}

impl<B: Backend> App<B> {
    pub fn new(backend: B, music_dir: PathBuf, door_source: TrackSource, door_title: String, door_duration: Option<Duration>) -> Self {
        Self {
            backend,
            mode: Mode::Background,
            background: None,
            door: Track::new(door_source, door_title, door_duration),
            bag: ShuffleBag::new(music_dir),
            failures: 0,
            no_music: false,
            next_track_at: None,
            next_open_at: None,
            last_space: None,
            stall: None,
            volume: 100,
        }
    }

    pub fn volume_up(&mut self) {
        self.set_volume(self.volume.saturating_add(VOLUME_STEP).min(100));
    }

    pub fn volume_down(&mut self) {
        self.set_volume(self.volume.saturating_sub(VOLUME_STEP));
    }

    fn set_volume(&mut self, volume: u8) {
        self.volume = volume;
        self.backend.set_master_gain(master_gain(volume));
    }

    /// Call for every SPACE press and auto-repeat. Returns whether it switched modes.
    pub fn on_space(&mut self, now: Instant) -> bool {
        let quiet = self.last_space.is_none_or(|last| now.duration_since(last) >= SPACE_QUIET_TIME);
        self.last_space = Some(now);
        if quiet {
            match self.mode {
                Mode::Background => self.start_door(),
                Mode::Door => self.return_to_background(),
            }
        }
        quiet
    }

    fn start_door(&mut self) {
        self.backend.set_paused(Slot::Background, true);
        self.mode = Mode::Door;
        self.door.position = Duration::ZERO;
        self.door.loaded = false;
        self.stall = None;
    }

    fn return_to_background(&mut self) {
        self.backend.stop(Slot::Door);
        self.door.loaded = false;
        self.door.position = Duration::ZERO;
        self.mode = Mode::Background;
        self.stall = None;
        self.backend.set_paused(Slot::Background, false);
    }

    /// Advances playback. Call often; it never blocks on audio output.
    pub fn tick(&mut self, now: Instant) {
        self.tend_device(now);
        if !self.backend.is_open() {
            return;
        }
        match self.mode {
            Mode::Door => self.tick_door(),
            Mode::Background => self.tick_background(now),
        }
        self.check_stall(now);
    }

    fn tend_device(&mut self, now: Instant) {
        if self.backend.is_open() {
            if self.backend.take_device_error() {
                self.lose_device();
            } else {
                return;
            }
        }
        if self.next_open_at.is_some_and(|at| now < at) {
            return;
        }
        match self.backend.open() {
            Ok(()) => self.next_open_at = None,
            Err(err) => {
                eprintln!("cannot open audio device: {err}");
                self.next_open_at = Some(now + DEVICE_RETRY);
            }
        }
    }

    /// Positions stay in the tracks, so playback resumes in place once the device is back.
    fn lose_device(&mut self) {
        self.backend.close();
        self.door.loaded = false;
        if let Some(track) = &mut self.background {
            track.loaded = false;
        }
        self.stall = None;
        self.next_open_at = None;
    }

    fn tick_door(&mut self) {
        if !self.door.loaded {
            match self.backend.load(Slot::Door, &self.door.source, self.door.position) {
                Ok(duration) => {
                    self.door.loaded = true;
                    self.door.duration = duration.or(self.door.duration);
                }
                Err(err) => {
                    eprintln!("cannot play door song: {err}");
                    self.return_to_background();
                }
            }
            return;
        }
        if self.backend.finished(Slot::Door) {
            self.return_to_background();
        } else {
            self.door.position = self.backend.position(Slot::Door);
        }
    }

    fn tick_background(&mut self, now: Instant) {
        let Some(track) = &mut self.background else {
            self.start_next_track(now);
            return;
        };
        if !track.loaded {
            let (source, at) = (track.source.clone(), track.position);
            match self.backend.load(Slot::Background, &source, at) {
                Ok(_) => track.loaded = true,
                Err(err) => {
                    eprintln!("cannot resume track: {err}");
                    self.background = None;
                }
            }
            return;
        }
        if self.backend.finished(Slot::Background) {
            if track.position < MIN_HEALTHY_PLAY {
                eprintln!("track ended immediately: {:?}", track.source);
                self.note_failure(now);
            }
            self.background = None;
            self.start_next_track(now);
            return;
        }
        track.position = self.backend.position(Slot::Background);
        if track.position >= MIN_HEALTHY_PLAY {
            self.failures = 0;
            self.no_music = false;
        }
    }

    fn start_next_track(&mut self, now: Instant) {
        if self.next_track_at.is_some_and(|at| now < at) {
            return;
        }
        self.next_track_at = None;
        let Some(path) = self.bag.next_track() else {
            self.no_music = true;
            self.next_track_at = Some(now + NO_MUSIC_RETRY);
            return;
        };
        let source = TrackSource::File(path.clone());
        match self.backend.load(Slot::Background, &source, Duration::ZERO) {
            Ok(duration) => {
                let mut track = Track::new(source, title_of(&path), duration);
                track.loaded = true;
                self.background = Some(track);
                self.stall = None;
            }
            Err(err) => {
                eprintln!("skipping track: {err}");
                self.note_failure(now);
            }
        }
    }

    /// Once every song in the folder has failed in a row, wait before trying again.
    fn note_failure(&mut self, now: Instant) {
        self.failures += 1;
        if self.failures >= self.bag.len().max(1) {
            self.failures = 0;
            self.no_music = true;
            self.next_track_at = Some(now + NO_MUSIC_RETRY);
        }
    }

    /// Catches outputs that silently stop without reporting an error.
    fn check_stall(&mut self, now: Instant) {
        let active = match self.mode {
            Mode::Door => self.door.loaded.then_some(Slot::Door),
            Mode::Background => {
                self.background.as_ref().is_some_and(|t| t.loaded).then_some(Slot::Background)
            }
        };
        let Some(slot) = active.filter(|slot| !self.backend.finished(*slot)) else {
            self.stall = None;
            return;
        };
        let position = self.backend.position(slot);
        match self.stall {
            Some((last, since)) if last == position => {
                if now.duration_since(since) >= STALL_TIMEOUT {
                    eprintln!("audio output stalled, reopening device");
                    self.lose_device();
                }
            }
            _ => self.stall = Some((position, now)),
        }
    }

    pub fn view(&self) -> View<'_> {
        let warning = if !self.backend.is_open() {
            Some("NO SOUND DEVICE - retrying...")
        } else if self.volume == 0 {
            Some("MUTED - press + to turn the volume up")
        } else if self.no_music && self.mode == Mode::Background {
            Some("NO PLAYABLE MUSIC - retrying...")
        } else {
            None
        };
        let track = match self.mode {
            Mode::Door => Some(&self.door),
            Mode::Background => self.background.as_ref(),
        };
        View {
            mode: self.mode,
            title: track.map(|t| t.title.as_str()),
            position: track.map_or(Duration::ZERO, |t| t.position),
            duration: track.and_then(|t| t.duration),
            warning,
            volume: self.volume,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::HashMap, fs};

    /// A scripted sound card.
    #[derive(Default)]
    struct Fake {
        open: bool,
        refuse_open: bool,
        device_error: bool,
        loaded: HashMap<&'static str, (TrackSource, Duration, bool)>,
        positions: HashMap<&'static str, Duration>,
        finished: HashMap<&'static str, bool>,
        broken: Vec<&'static str>,
        loads: usize,
        gain: f32,
    }

    fn key(slot: Slot) -> &'static str {
        match slot {
            Slot::Background => "bg",
            Slot::Door => "door",
        }
    }

    impl Backend for Fake {
        fn open(&mut self) -> Result<(), String> {
            if self.refuse_open {
                return Err("no device".into());
            }
            self.close();
            self.open = true;
            Ok(())
        }
        fn close(&mut self) {
            self.open = false;
            self.loaded.clear();
            self.positions.clear();
        }
        fn is_open(&self) -> bool {
            self.open
        }
        fn take_device_error(&mut self) -> bool {
            std::mem::take(&mut self.device_error)
        }
        fn load(&mut self, slot: Slot, source: &TrackSource, at: Duration) -> Result<Option<Duration>, String> {
            self.loads += 1;
            if let TrackSource::File(path) = source {
                let name = path.file_name().unwrap().to_string_lossy();
                if self.broken.iter().any(|b| *b == name) {
                    return Err("broken".into());
                }
            }
            self.loaded.insert(key(slot), (source.clone(), at, false));
            self.positions.insert(key(slot), at);
            self.finished.insert(key(slot), false);
            Ok(Some(Duration::from_secs(100)))
        }
        fn set_paused(&mut self, slot: Slot, paused: bool) {
            if let Some(entry) = self.loaded.get_mut(key(slot)) {
                entry.2 = paused;
            }
        }
        fn set_master_gain(&mut self, gain: f32) {
            self.gain = gain;
        }
        fn stop(&mut self, slot: Slot) {
            self.loaded.remove(key(slot));
            self.positions.remove(key(slot));
        }
        fn position(&self, slot: Slot) -> Duration {
            self.positions.get(key(slot)).copied().unwrap_or_default()
        }
        fn finished(&self, slot: Slot) -> bool {
            !self.loaded.contains_key(key(slot)) || self.finished.get(key(slot)).copied().unwrap_or(false)
        }
    }

    struct Rig {
        app: App<Fake>,
        now: Instant,
        dir: PathBuf,
    }

    impl Drop for Rig {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    impl Rig {
        fn new(name: &str, files: &[&str]) -> Self {
            let dir = std::env::temp_dir().join(format!("halloweener-app-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            for file in files {
                fs::write(dir.join(file), b"x").unwrap();
            }
            let door = TrackSource::Memory { bytes: Vec::new().into(), extension: None };
            let app = App::new(Fake::default(), dir.clone(), door, "Spooky".into(), None);
            Self { app, now: Instant::now(), dir }
        }

        /// Moves time forward, with playing tracks advancing so no stall is detected.
        fn advance(&mut self, by: Duration) {
            let step = Duration::from_millis(500);
            let mut left = by;
            while !left.is_zero() {
                let delta = left.min(step);
                self.now += delta;
                left -= delta;
                let active = key(match self.app.mode {
                    Mode::Door => Slot::Door,
                    Mode::Background => Slot::Background,
                });
                if let Some(position) = self.app.backend.positions.get_mut(active) {
                    *position += delta;
                }
                self.tick();
            }
        }

        fn tick(&mut self) {
            let now = self.now;
            self.app.tick(now);
        }

        fn space(&mut self) -> bool {
            self.now += Duration::from_secs(1);
            let now = self.now;
            let switched = self.app.on_space(now);
            self.tick();
            switched
        }

        fn bg_paused(&self) -> bool {
            self.app.backend.loaded["bg"].2
        }
    }

    const SEC: Duration = Duration::from_secs(1);

    #[test]
    fn starts_in_background_mode_playing_a_track() {
        let mut rig = Rig::new("start", &["a.mp3"]);
        rig.tick();
        rig.tick();
        let view = rig.app.view();
        assert_eq!(view.mode, Mode::Background);
        assert_eq!(view.title, Some("a"));
        assert!(!rig.bg_paused());
    }

    #[test]
    fn space_pauses_background_and_second_space_resumes_same_spot() {
        let mut rig = Rig::new("toggle", &["a.mp3", "b.mp3"]);
        rig.tick();
        rig.advance(SEC * 30);
        let title = rig.app.view().title.unwrap().to_string();
        let position = rig.app.view().position;

        assert!(rig.space());
        assert_eq!(rig.app.view().mode, Mode::Door);
        assert_eq!(rig.app.view().title, Some("Spooky"));
        assert!(rig.bg_paused());
        assert!(rig.app.backend.loaded.contains_key("door"));

        rig.advance(SEC * 10);
        assert!(rig.space());
        assert_eq!(rig.app.view().mode, Mode::Background);
        assert!(!rig.bg_paused());
        assert_eq!(rig.app.view().title.unwrap(), title);
        assert_eq!(rig.app.backend.position(Slot::Background), position);
        assert!(!rig.app.backend.loaded.contains_key("door"));
    }

    #[test]
    fn door_song_ending_resumes_background() {
        let mut rig = Rig::new("end", &["a.mp3"]);
        rig.tick();
        rig.advance(SEC * 5);
        rig.space();
        rig.advance(SEC * 5);
        rig.app.backend.finished.insert("door", true);
        rig.tick();
        assert_eq!(rig.app.view().mode, Mode::Background);
        assert!(!rig.bg_paused());
    }

    #[test]
    fn door_song_restarts_from_the_top_each_time() {
        let mut rig = Rig::new("restart", &["a.mp3"]);
        rig.tick();
        rig.space();
        rig.advance(SEC * 20);
        rig.space();
        rig.space();
        assert_eq!(rig.app.backend.loaded["door"].1, Duration::ZERO);
        assert_eq!(rig.app.view().position, Duration::ZERO);
    }

    #[test]
    fn holding_or_double_tapping_space_only_switches_once() {
        let mut rig = Rig::new("hold", &["a.mp3"]);
        rig.tick();
        let start = rig.now;
        assert!(rig.app.on_space(start));
        // First auto-repeat after the OS delay, then a stream of repeats for ten seconds.
        let mut t = start + Duration::from_millis(600);
        while t < start + SEC * 10 {
            assert!(!rig.app.on_space(t));
            t += Duration::from_millis(30);
        }
        assert_eq!(rig.app.view().mode, Mode::Door);
        // Released and pressed again later: works.
        assert!(rig.app.on_space(t + SPACE_QUIET_TIME));
        assert_eq!(rig.app.view().mode, Mode::Background);
    }

    #[test]
    fn finished_track_moves_on_to_another() {
        let mut rig = Rig::new("next", &["a.mp3", "b.mp3"]);
        rig.tick();
        rig.advance(SEC * 10);
        let first = rig.app.view().title.unwrap().to_string();
        rig.app.backend.finished.insert("bg", true);
        rig.tick();
        assert_ne!(rig.app.view().title.unwrap(), first);
        assert_eq!(rig.app.view().position, Duration::ZERO);
    }

    #[test]
    fn broken_files_are_skipped() {
        let mut rig = Rig::new("broken", &["bad1.mp3", "bad2.mp3", "good.mp3"]);
        rig.app.backend.broken = vec!["bad1.mp3", "bad2.mp3"];
        for _ in 0..4 {
            rig.tick();
        }
        assert_eq!(rig.app.view().title, Some("good"));
    }

    #[test]
    fn all_broken_warns_backs_off_and_door_still_works() {
        let mut rig = Rig::new("allbad", &["a.mp3", "b.mp3"]);
        rig.app.backend.broken = vec!["a.mp3", "b.mp3"];
        for _ in 0..1000 {
            rig.tick();
        }
        assert_eq!(rig.app.backend.loads, 2, "must not hammer broken files");
        assert!(rig.app.view().warning.is_some());
        rig.space();
        assert_eq!(rig.app.view().mode, Mode::Door);
        assert!(rig.app.backend.loaded.contains_key("door"));
    }

    #[test]
    fn folder_emptied_at_runtime_recovers_when_music_returns() {
        let mut rig = Rig::new("empty", &[]);
        rig.tick();
        rig.tick();
        assert!(rig.app.view().warning.is_some());
        fs::write(rig.dir.join("new.mp3"), b"x").unwrap();
        rig.advance(NO_MUSIC_RETRY + SEC);
        assert_eq!(rig.app.view().title, Some("new"));
        rig.advance(SEC * 2);
        assert!(rig.app.view().warning.is_none());
    }

    #[test]
    fn device_error_reopens_and_resumes_at_same_position() {
        let mut rig = Rig::new("device", &["a.mp3"]);
        rig.tick();
        rig.advance(SEC * 42);
        let position = rig.app.view().position;
        rig.app.backend.device_error = true;
        rig.tick(); // reopens, then reloads
        rig.tick();
        assert_eq!(rig.app.view().title, Some("a"));
        assert_eq!(rig.app.backend.loaded["bg"].1, position);
    }

    #[test]
    fn missing_device_warns_and_door_starts_when_it_appears() {
        let mut rig = Rig::new("nodevice", &["a.mp3"]);
        rig.app.backend.refuse_open = true;
        rig.tick();
        assert!(rig.app.view().warning.is_some());
        rig.space();
        assert_eq!(rig.app.view().mode, Mode::Door);

        rig.app.backend.refuse_open = false;
        rig.advance(DEVICE_RETRY + SEC);
        assert!(rig.app.view().warning.is_none());
        assert!(rig.app.backend.loaded.contains_key("door"));
    }

    #[test]
    fn device_lost_during_door_song_restores_both_positions() {
        let mut rig = Rig::new("doorloss", &["a.mp3"]);
        rig.tick();
        rig.advance(SEC * 30);
        let bg_position = rig.app.view().position;
        rig.space();
        rig.advance(SEC * 20);
        let door_position = rig.app.view().position;

        rig.app.backend.device_error = true;
        rig.tick();
        rig.tick();
        assert_eq!(rig.app.view().mode, Mode::Door);
        assert_eq!(rig.app.backend.loaded["door"].1, door_position);

        rig.space();
        rig.tick();
        assert_eq!(rig.app.backend.loaded["bg"].1, bg_position);
        assert!(!rig.bg_paused());
    }

    #[test]
    fn frozen_position_is_treated_as_a_dead_device() {
        let mut rig = Rig::new("stall", &["a.mp3"]);
        rig.tick();
        rig.tick();
        rig.advance(SEC * 5);
        let position = rig.app.view().position;
        let opened = rig.app.backend.open;
        assert!(opened);
        // Time passes, position does not.
        for _ in 0..12 {
            rig.now += Duration::from_millis(500);
            rig.tick();
        }
        assert_eq!(rig.app.backend.loaded["bg"].1, position);
    }

    #[test]
    fn volume_keys_step_and_stop_at_the_limits() {
        let mut rig = Rig::new("volume", &["a.mp3"]);
        assert_eq!(rig.app.view().volume, 100);
        rig.app.volume_up();
        assert_eq!(rig.app.view().volume, 100);
        rig.app.volume_down();
        assert_eq!(rig.app.view().volume, 95);
        for _ in 0..30 {
            rig.app.volume_down();
        }
        assert_eq!(rig.app.view().volume, 0);
        assert_eq!(rig.app.backend.gain, 0.0);
        rig.app.volume_up();
        assert_eq!(rig.app.view().volume, 5);
        assert!(rig.app.backend.gain > 0.0, "turning it up un-mutes");
    }

    #[test]
    fn half_volume_is_a_quarter_of_the_signal_level() {
        let mut rig = Rig::new("gain", &["a.mp3"]);
        for _ in 0..10 {
            rig.app.volume_down();
        }
        assert_eq!(rig.app.backend.gain, 0.25);
    }

    #[test]
    fn muting_shows_a_warning_on_both_screens() {
        let mut rig = Rig::new("mute", &["a.mp3"]);
        rig.tick();
        assert!(rig.app.view().warning.is_none());
        for _ in 0..20 {
            rig.app.volume_down();
        }
        assert!(rig.app.view().warning.unwrap().contains("MUTED"));
        rig.space();
        assert!(rig.app.view().warning.unwrap().contains("MUTED"));
    }

    #[test]
    fn paused_background_does_not_count_as_stalled() {
        let mut rig = Rig::new("paused", &["a.mp3"]);
        rig.tick();
        rig.space();
        let loads = rig.app.backend.loads;
        rig.advance(SEC * 60);
        assert_eq!(rig.app.backend.loads, loads);
    }
}
