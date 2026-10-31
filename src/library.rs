//! Finds music files and hands them out in shuffled order, with every track
//! played once before any repeats.

use rand::seq::SliceRandom;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub const EXTENSIONS: &[&str] = &["mp3", "flac", "ogg", "oga", "wav", "m4a", "mp4"];
const MAX_DEPTH: usize = 16; // also stops symlink loops

/// Lists the tracks under a folder. A single file is a one-track list, which then
/// loops forever; it was named explicitly, so its extension doesn't matter.
pub fn scan(root: &Path) -> io::Result<Vec<PathBuf>> {
    if fs::metadata(root)?.is_file() {
        return Ok(vec![root.to_path_buf()]);
    }
    let mut found = Vec::new();
    let mut pending = vec![(root.to_path_buf(), 0)];
    let mut first = true;
    while let Some((dir, depth)) = pending.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(err) if first => return Err(err),
            Err(_) => continue, // one unreadable subfolder must not stop the music
        };
        first = false;
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            if name.to_string_lossy().starts_with('.') {
                continue; // hidden files, including macOS "._song.mp3" droppings
            }
            match fs::metadata(&path) {
                Ok(meta) if meta.is_dir() && depth < MAX_DEPTH => pending.push((path, depth + 1)),
                Ok(meta) if meta.is_file() && is_music(&path) => found.push(path),
                _ => {}
            }
        }
    }
    Ok(found)
}

fn is_music(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| EXTENSIONS.iter().any(|known| ext.eq_ignore_ascii_case(known)))
}

/// Shows a filename as a song title.
pub fn title_of(path: &Path) -> String {
    let stem = path.file_stem().unwrap_or(path.as_os_str()).to_string_lossy();
    stem.chars()
        .filter(|c| !c.is_control())
        .map(|c| if c == '_' { ' ' } else { c })
        .collect()
}

pub struct ShuffleBag {
    root: PathBuf,
    all: Vec<PathBuf>,
    queue: Vec<PathBuf>,
    last: Option<PathBuf>,
}

impl ShuffleBag {
    pub fn new(root: PathBuf) -> Self {
        Self { root, all: Vec::new(), queue: Vec::new(), last: None }
    }

    /// Number of tracks known from the most recent scan.
    pub fn len(&self) -> usize {
        self.all.len()
    }

    pub fn next_track(&mut self) -> Option<PathBuf> {
        if self.queue.is_empty() {
            self.refill();
        }
        let track = self.queue.pop()?;
        self.last = Some(track.clone());
        Some(track)
    }

    /// Rescans on every round, so songs can be added while the app runs. A failed
    /// scan (e.g. a USB stick hiccup) keeps the previous list.
    fn refill(&mut self) {
        match scan(&self.root) {
            Ok(found) => self.all = found,
            Err(err) => eprintln!("scan of {} failed: {err}", self.root.display()),
        }
        self.queue = self.all.clone();
        self.queue.shuffle(&mut rand::rng());
        // `pop` takes from the end: never play the same song twice in a row.
        if self.queue.len() > 1 && self.queue.last() == self.last.as_ref() {
            self.queue.swap(0, self.all.len() - 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("halloweener-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn scan_finds_music_recursively_and_skips_the_rest() {
        let dir = temp_dir("scan");
        fs::create_dir(dir.join("sub")).unwrap();
        for name in ["a.mp3", "B.FLAC", "sub/c.ogg", "cover.jpg", "._a.mp3", ".hidden.mp3"] {
            fs::write(dir.join(name), b"x").unwrap();
        }
        assert_eq!(scan(&dir).unwrap().len(), 3);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn single_file_loops_and_survives_vanishing_for_a_moment() {
        let dir = temp_dir("file");
        let file = dir.join("loop.weird");
        fs::write(&file, b"x").unwrap();
        assert_eq!(scan(&file).unwrap(), vec![file.clone()]);

        let mut bag = ShuffleBag::new(file.clone());
        for _ in 0..5 {
            assert_eq!(bag.next_track().as_ref(), Some(&file));
        }
        fs::remove_file(&file).unwrap();
        assert!(scan(&file).is_err());
        assert_eq!(bag.next_track().as_ref(), Some(&file), "keeps the last good list");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn bag_plays_everything_before_repeating_and_never_twice_in_a_row() {
        let dir = temp_dir("bag");
        for name in ["a.mp3", "b.mp3", "c.mp3"] {
            fs::write(dir.join(name), b"x").unwrap();
        }
        let mut bag = ShuffleBag::new(dir.clone());
        let mut previous = None;
        for _ in 0..100 {
            let round: Vec<_> = (0..3).map(|_| bag.next_track().unwrap()).collect();
            assert_eq!(round.iter().collect::<HashSet<_>>().len(), 3);
            assert_ne!(previous.as_ref(), round.first());
            previous = round.last().cloned();
        }
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn single_track_keeps_looping_and_empty_folder_gives_none() {
        let dir = temp_dir("one");
        let mut bag = ShuffleBag::new(dir.clone());
        assert_eq!(bag.next_track(), None);
        fs::write(dir.join("a.mp3"), b"x").unwrap();
        for _ in 0..5 {
            assert!(bag.next_track().is_some());
        }
        fs::remove_dir_all(&dir).unwrap();
    }
}
