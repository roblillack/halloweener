//! Audio output on top of rodio.
//!
//! Rules that keep the UI thread from ever blocking on the audio thread:
//! * every track gets a fresh `Player`; `Player::clear`, `try_seek`, and `append`
//!   after `stop` all wait for the audio callback, and hang if the device is dead.
//! * seeking happens on the decoder, before playback starts.

use rodio::{
    cpal, ChannelCount, Decoder, DeviceSinkBuilder, MixerDeviceSink, Player, SampleRate, Source,
};
use std::{
    fs::File,
    io::{BufReader, Cursor, Read, Seek},
    panic::{catch_unwind, AssertUnwindSafe},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Background,
    Door,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TrackSource {
    File(PathBuf),
    /// Kept in RAM, so the door song still works if its file becomes unavailable.
    Memory { bytes: Arc<[u8]>, extension: Option<String> },
}

/// What the app needs from audio output. A trait, so playback logic can be tested
/// without a sound card.
pub trait Backend {
    fn open(&mut self) -> Result<(), String>;
    fn close(&mut self);
    fn is_open(&self) -> bool;
    /// True once after the device reported a fatal error.
    fn take_device_error(&mut self) -> bool;
    /// Replaces whatever is in `slot`. Returns the track length, if known.
    fn load(&mut self, slot: Slot, source: &TrackSource, at: Duration) -> Result<Option<Duration>, String>;
    fn set_paused(&mut self, slot: Slot, paused: bool);
    fn stop(&mut self, slot: Slot);
    fn position(&self, slot: Slot) -> Duration;
    fn finished(&self, slot: Slot) -> bool;
}

/// Decodes a track once, to reject broken door songs at startup.
pub fn probe(source: &TrackSource) -> Result<Option<Duration>, String> {
    Ok(decode(source)?.total_duration())
}

trait AnyReader: Read + Seek + Send + Sync {}
impl<T: Read + Seek + Send + Sync> AnyReader for T {}

struct Reader(Box<dyn AnyReader>);
impl Read for Reader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0.read(buf)
    }
}
impl Seek for Reader {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        self.0.seek(pos)
    }
}

fn decode(source: &TrackSource) -> Result<Decoder<Reader>, String> {
    let (reader, len, extension): (Box<dyn AnyReader>, u64, Option<String>) = match source {
        TrackSource::File(path) => {
            let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
            let len = file.metadata().map_err(|e| format!("{}: {e}", path.display()))?.len();
            let extension = path.extension().map(|ext| ext.to_string_lossy().to_lowercase());
            (Box::new(BufReader::new(file)), len, extension)
        }
        TrackSource::Memory { bytes, extension } => {
            (Box::new(Cursor::new(bytes.clone())), bytes.len() as u64, extension.clone())
        }
    };
    let reader = Reader(reader);
    catch_unwind(AssertUnwindSafe(move || {
        let mut builder = Decoder::builder()
            .with_data(reader)
            .with_byte_len(len)
            .with_seekable(true);
        if let Some(extension) = &extension {
            builder = builder.with_hint(extension);
        }
        builder.build()
    }))
    .map_err(|_| "decoder panicked while opening the file".to_string())?
    .map_err(|e| e.to_string())
}

/// Ends the track, rather than the audio thread, if a decoder panics on damaged data.
struct PanicSafe<S> {
    inner: S,
    dead: bool,
}

impl<S: Source> Iterator for PanicSafe<S> {
    type Item = S::Item;
    fn next(&mut self) -> Option<Self::Item> {
        if self.dead {
            return None;
        }
        match catch_unwind(AssertUnwindSafe(|| self.inner.next())) {
            Ok(sample) => sample,
            Err(_) => {
                self.dead = true;
                None
            }
        }
    }
}

impl<S: Source> Source for PanicSafe<S> {
    fn current_span_len(&self) -> Option<usize> {
        if self.dead { Some(0) } else { self.inner.current_span_len() }
    }
    fn channels(&self) -> ChannelCount {
        self.inner.channels()
    }
    fn sample_rate(&self) -> SampleRate {
        self.inner.sample_rate()
    }
    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
}

struct Playing {
    player: Player,
    /// Where in the track this player started; `Player::get_pos` counts from zero.
    offset: Duration,
}

pub struct Rodio {
    background_volume: f32,
    sink: Option<MixerDeviceSink>,
    device_error: Option<Arc<AtomicBool>>,
    background: Option<Playing>,
    door: Option<Playing>,
}

impl Rodio {
    /// `background_volume` is a linear factor; the door song always plays at 1.0.
    pub fn new(background_volume: f32) -> Self {
        Self { background_volume, sink: None, device_error: None, background: None, door: None }
    }

    fn slot(&self, slot: Slot) -> &Option<Playing> {
        match slot {
            Slot::Background => &self.background,
            Slot::Door => &self.door,
        }
    }

    fn slot_mut(&mut self, slot: Slot) -> &mut Option<Playing> {
        match slot {
            Slot::Background => &mut self.background,
            Slot::Door => &mut self.door,
        }
    }
}

impl Backend for Rodio {
    fn open(&mut self) -> Result<(), String> {
        self.close();
        let flag = Arc::new(AtomicBool::new(false));
        let on_error = flag.clone();
        let mut sink = DeviceSinkBuilder::from_default_device()
            .map_err(|e| e.to_string())?
            .with_error_callback(move |err: cpal::Error| {
                // Underruns and the like are not fatal, and would flood the log.
                if matches!(
                    err.kind(),
                    cpal::ErrorKind::DeviceNotAvailable | cpal::ErrorKind::StreamInvalidated
                ) {
                    eprintln!("audio device error: {err}");
                    on_error.store(true, Ordering::SeqCst);
                }
            })
            .open_sink_or_fallback()
            .map_err(|e| e.to_string())?;
        sink.log_on_drop(false);
        self.sink = Some(sink);
        self.device_error = Some(flag);
        Ok(())
    }

    fn close(&mut self) {
        self.background = None;
        self.door = None;
        self.sink = None;
        self.device_error = None;
    }

    fn is_open(&self) -> bool {
        self.sink.is_some()
    }

    fn take_device_error(&mut self) -> bool {
        self.device_error.as_ref().is_some_and(|flag| flag.swap(false, Ordering::SeqCst))
    }

    fn load(&mut self, slot: Slot, source: &TrackSource, at: Duration) -> Result<Option<Duration>, String> {
        let sink = self.sink.as_ref().ok_or("audio device is not open")?;
        let mut decoder = decode(source)?;
        let duration = decoder.total_duration();
        let mut offset = Duration::ZERO;
        if !at.is_zero() {
            // A failed seek replays the song from the start rather than skipping it.
            match catch_unwind(AssertUnwindSafe(|| decoder.try_seek(at))) {
                Ok(Ok(())) => offset = at,
                Ok(Err(err)) => eprintln!("seek failed, restarting track: {err}"),
                Err(_) => return Err("decoder panicked while seeking".into()),
            }
        }
        let player = Player::connect_new(sink.mixer());
        if slot == Slot::Background {
            player.set_volume(self.background_volume);
        }
        player.append(PanicSafe { inner: decoder, dead: false });
        *self.slot_mut(slot) = Some(Playing { player, offset });
        Ok(duration)
    }

    fn set_paused(&mut self, slot: Slot, paused: bool) {
        if let Some(playing) = self.slot(slot) {
            if paused {
                playing.player.pause();
            } else {
                playing.player.play();
            }
        }
    }

    fn stop(&mut self, slot: Slot) {
        // Dropping a Player stops it without waiting for the audio thread.
        *self.slot_mut(slot) = None;
    }

    fn position(&self, slot: Slot) -> Duration {
        self.slot(slot)
            .as_ref()
            .map_or(Duration::ZERO, |playing| playing.offset + playing.player.get_pos())
    }

    fn finished(&self, slot: Slot) -> bool {
        self.slot(slot).as_ref().is_none_or(|playing| playing.player.empty())
    }
}
