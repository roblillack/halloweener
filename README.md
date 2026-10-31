# Halloweener

Fullscreen terminal music player for a Halloween door kiosk.

- **Dark green screen with a skull and glowing eyes:** background music, either a folder on shuffle or one looping file.
- **SPACE or a mouse click:** pauses it and plays the door song on a **red screen** with SPOOKY, SCARY and SKELETONS taking turns in large wobbling letters.
- **SPACE or a click again, or the door song ending:** background music continues exactly where it stopped.

## Run

```sh
sudo apt install libasound2-dev pkg-config   # build dependencies on Debian/Ubuntu
cargo build --release
./target/release/halloweener ~/Music/halloween ~/Music/spooky-scary-skeletons.mp3
```

The first argument is a folder or a single file, which then loops forever. Plays mp3,
flac, ogg, wav, m4a and mp4. Subfolders are included, and songs added while it runs
join the next shuffle round. The door song is loaded into memory at startup.

## Background volume

Background music plays at 33% of the door song's level. To change it:

```sh
halloweener --background-volume 50 ~/Music/halloween ~/Music/spooky-scary-skeletons.mp3
```

The value is 0-100 and scales the signal level, so 33 is about 10 dB quieter, which
most people hear as roughly half as loud.

## Master volume

`+` or `=` turns everything up and `-` turns it down, in 5% steps, on either screen.
The current value is always on screen. It keeps the balance between background and
door song, and can only go down from the system volume, so set the system volume to
the loudest you'd ever want. Each step is an even change by ear: 50% is a quarter of
the signal level, about 12 dB quieter.

At 0% a yellow MUTED warning appears, until `+` turns it back up. The volume isn't
saved: every start, including an automatic restart after a crash, begins at 100%.

## Keys

| Key      | Action                                                |
|----------|-------------------------------------------------------|
| `SPACE` or click | Start the door song / go back to background music |
| `+`, `=` | Volume up                                              |
| `-`      | Volume down                                            |
| `Ctrl+Q` | Quit. Deliberately not shown on screen                 |

Everything else, including `Ctrl+C` and scrolling, is ignored. SPACE and clicks only
register after 0.5 s of quiet, so a double-click switches once. Holding SPACE also
switches once, as long as the keyboard's repeat delay is under 0.5 s. With a longer
delay it switches twice, so set it to about 400 ms on the kiosk.

## What happens when things go wrong

| Problem                             | Behaviour                                             |
|-------------------------------------|-------------------------------------------------------|
| Bad setup at launch                 | Clear error message, exit code 2, no restart           |
| Damaged or unreadable music file    | Skipped                                                |
| No playable music at runtime        | Yellow warning, retries every 5 s; door song still works |
| Sound device or PipeWire goes away  | Yellow warning, retries, resumes at the same position  |
| Audio silently stalls               | Detected after 4 s, device reopened, resumes in place  |
| Player crashes or hangs for 15 s    | Built-in supervisor restarts it after about 1 s        |

Errors are logged to `~/.local/state/halloweener.log`. A restart begins a new
background track rather than resuming the old one.

## Kiosk checklist for the laptop

The player can't control these, so set them up beforehand:

- Disable suspend and lid-close actions, screen blanking and the lock screen.
- Keep the laptop on mains power, and test the volume at the door.
- Keep the music on the internal disk rather than a USB stick.
- Make sure the terminal window has keyboard focus, and start it with something like
  `kitty --start-as=fullscreen halloweener <music-folder> <door-song>`.
