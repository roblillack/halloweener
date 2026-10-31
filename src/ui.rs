//! The two screens: dark green while background music plays, red for the door song.

use crate::app::{Mode, View};
use ratatui::{
    layout::Alignment,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use std::time::Duration;
use unicode_width::UnicodeWidthChar;

// 256-color palette entries, which unlike ANSI 0-15 are not changed by terminal themes.
const WHITE: Color = Color::Indexed(231);
const YELLOW: Color = Color::Indexed(226);
const BLACK: Color = Color::Indexed(16);
const GREEN_BG: Color = Color::Indexed(22);
const GREEN_LIGHT: Color = Color::Indexed(120);
const RED_BG: Color = Color::Indexed(160);
const RED_SHADOW: Color = Color::Indexed(52);

const ART_HEIGHT: usize = 6;

type Glyph = [&'static str; ART_HEIGHT];

const S: Glyph = ["███████╗", "██╔════╝", "███████╗", "╚════██║", "███████║", "╚══════╝"];
const P: Glyph = ["██████╗ ", "██╔══██╗", "██████╔╝", "██╔═══╝ ", "██║     ", "╚═╝     "];
const O: Glyph = [" ██████╗ ", "██╔═══██╗", "██║   ██║", "██║   ██║", "╚██████╔╝", " ╚═════╝ "];
const K: Glyph = ["██╗  ██╗", "██║ ██╔╝", "█████╔╝ ", "██╔═██╗ ", "██║  ██╗", "╚═╝  ╚═╝"];
const Y: Glyph = ["██╗   ██╗", "╚██╗ ██╔╝", " ╚████╔╝ ", "  ╚██╔╝  ", "   ██║   ", "   ╚═╝   "];
const C: Glyph = [" ██████╗", "██╔════╝", "██║     ", "██║     ", "╚██████╗", " ╚═════╝"];
const A: Glyph = [" █████╗ ", "██╔══██╗", "███████║", "██╔══██║", "██║  ██║", "╚═╝  ╚═╝"];
const R: Glyph = ["██████╗ ", "██╔══██╗", "██████╔╝", "██╔══██╗", "██║  ██║", "╚═╝  ╚═╝"];
const E: Glyph = ["███████╗", "██╔════╝", "█████╗  ", "██╔══╝  ", "███████╗", "╚══════╝"];
const L: Glyph = ["██╗     ", "██║     ", "██║     ", "██║     ", "███████╗", "╚══════╝"];
const T: Glyph = ["████████╗", "╚══██╔══╝", "   ██║   ", "   ██║   ", "   ██║   ", "   ╚═╝   "];
const N: Glyph = ["███╗   ██╗", "████╗  ██║", "██╔██╗ ██║", "██║╚██╗██║", "██║ ╚████║", "╚═╝  ╚═══╝"];

struct Word {
    name: &'static str,
    letters: &'static [Glyph],
}

const WORDS: [Word; 3] = [
    Word { name: "SPOOKY", letters: &[S, P, O, O, K, Y] },
    Word { name: "SCARY", letters: &[S, C, A, R, Y] },
    Word { name: "SKELETONS", letters: &[S, K, E, L, E, T, O, N, S] },
];
const WORD_TIME_MILLIS: u128 = 750;
const WOBBLE_PERIOD_SECS: f32 = 0.8;
/// How far the wave has moved on from one letter to the next, in radians.
const WOBBLE_LETTER_PHASE: f32 = 0.9;

const DIGITS: [[&str; ART_HEIGHT]; 10] = [
    [" ██████╗ ", "██╔═████╗", "██║██╔██║", "████╔╝██║", "╚██████╔╝", " ╚═════╝ "],
    ["   ██╗   ", "  ███║   ", "  ╚██║   ", "   ██║   ", "   ██║   ", "   ╚═╝   "],
    ["██████╗  ", "╚════██╗ ", " █████╔╝ ", "██╔═══╝  ", "███████╗ ", "╚══════╝ "],
    ["██████╗  ", "╚════██╗ ", " █████╔╝ ", " ╚═══██╗ ", "██████╔╝ ", "╚═════╝  "],
    ["██╗  ██╗ ", "██║  ██║ ", "███████║ ", "╚════██║ ", "     ██║ ", "     ╚═╝ "],
    ["███████╗ ", "██╔════╝ ", "███████╗ ", "╚════██║ ", "███████║ ", "╚══════╝ "],
    [" ██████╗ ", "██╔════╝ ", "███████╗ ", "██╔═══██╗", "╚██████╔╝", " ╚═════╝ "],
    ["███████╗ ", "╚════██║ ", "    ██╔╝ ", "   ██╔╝  ", "   ██║   ", "   ╚═╝   "],
    [" █████╗  ", "██╔══██╗ ", "╚█████╔╝ ", "██╔══██╗ ", "╚█████╔╝ ", " ╚════╝  "],
    [" █████╗  ", "██╔══██╗ ", "╚██████║ ", " ╚═══██║ ", " █████╔╝ ", " ╚════╝  "],
];
const COLON: [&str; ART_HEIGHT] = ["    ", "██╗ ", "╚═╝ ", "██╗ ", "╚═╝ ", "    "];

/// One character per pixel: X is bone, o is an eye. A pixel is two cells wide,
/// because terminal cells are about twice as tall as they are wide.
const SKULL: [&str; 13] = [
    ".....XXXXX.....",
    "...XXXXXXXXX...",
    "..XXXXXXXXXXX..",
    ".XXXXXXXXXXXXX.",
    ".XXXXXXXXXXXXX.",
    ".XXoooXXXoooXX.",
    ".XXoooXXXoooXX.",
    ".XXoooXXXoooXX.",
    "..XXXXX.XXXXX..",
    "...XXXX.XXXX...",
    "....XXXXXXX....",
    "....X.X.X.X....",
    "....X.X.X.X....",
];
const MAX_SKULL_SCALE: usize = 3;

const SKULL_PIXEL_WIDTH: usize = 2;
/// Centers of the eye sockets, in skull pixels.
const EYES: [(f32, f32); 2] = [(4.5, 6.5), (10.5, 6.5)];
/// Dark red through bright red to yellow.
const EYE_RAMP: [u8; 10] = [52, 88, 124, 160, 196, 202, 208, 214, 220, 226];
/// White through ever stronger pink, for bone lit by the eyes.
const HALO_RAMP: [u8; 5] = [231, 224, 217, 210, 203];
/// How far the glow reaches onto the bone, measured from an eye's center.
const HALO_START: f32 = 1.5;
const HALO_REACH: f32 = 2.6;

/// Eye brightness from 0 to 1: a slow breath with some uneven flicker on top.
/// Everything here stays below 1 Hz, so it smolders rather than strobes.
fn eye_glow(elapsed: Duration) -> f32 {
    let t = elapsed.as_secs_f32();
    let breath = 0.5 + 0.5 * (t * std::f32::consts::TAU / 3.0).sin();
    let flicker = 0.5 + 0.5 * (t * 5.3).sin() * (t * 1.7).sin();
    (0.75 * breath + 0.25 * flicker).clamp(0.0, 1.0)
}

fn ramp(colors: &[u8], position: f32) -> Color {
    let last = colors.len() - 1;
    Color::Indexed(colors[((position.clamp(0.0, 1.0) * last as f32).round() as usize).min(last)])
}

/// Color of one cell of the skull, or None where the skull has a hole.
fn skull_cell(pixel: u8, x: f32, y: f32, glow: f32) -> Option<Color> {
    let distance = EYES
        .iter()
        .map(|(eye_x, eye_y)| (x - eye_x).hypot(y - eye_y))
        .fold(f32::MAX, f32::min);
    match pixel {
        b'X' => {
            let reach = 1.0 - ((distance - HALO_START) / HALO_REACH);
            Some(ramp(&HALO_RAMP, glow * reach))
        }
        b'o' => {
            let heat = (0.3 + 0.7 * glow) * (1.0 - 0.6 * distance / 2.1);
            Some(ramp(&EYE_RAMP, heat))
        }
        _ => None,
    }
}

fn skull_lines(scale: usize, glow: f32) -> Vec<Line<'static>> {
    let scale = scale.max(1);
    let columns = SKULL[0].len() * SKULL_PIXEL_WIDTH * scale;
    let mut lines = Vec::with_capacity(SKULL.len() * scale);
    for row in 0..SKULL.len() * scale {
        let pixels = SKULL[row / scale].as_bytes();
        let y = (row as f32 + 0.5) / scale as f32;
        let mut spans = Vec::new();
        let mut run = String::new();
        let mut run_color = None;
        for column in 0..columns {
            let pixel = pixels[column / (SKULL_PIXEL_WIDTH * scale)];
            let x = (column as f32 + 0.5) / (SKULL_PIXEL_WIDTH * scale) as f32;
            let color = skull_cell(pixel, x, y, glow);
            if color != run_color && !run.is_empty() {
                spans.push(skull_span(std::mem::take(&mut run), run_color));
            }
            run_color = color;
            run.push(if color.is_some() { '█' } else { ' ' });
        }
        spans.push(skull_span(run, run_color));
        lines.push(Line::from(spans));
    }
    lines
}

fn skull_span(text: String, color: Option<Color>) -> Span<'static> {
    match color {
        Some(color) => Span::styled(text, Style::new().fg(color)),
        None => Span::raw(text),
    }
}

pub fn format_time(time: Duration) -> String {
    let total = time.as_secs();
    let (hours, minutes, seconds) = (total / 3600, total / 60 % 60, total % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

/// Builds the time in big digits. All digits have one width, so it doesn't jiggle.
fn big_time(text: &str) -> Vec<String> {
    (0..ART_HEIGHT)
        .map(|row| {
            text.chars()
                .map(|c| match c.to_digit(10) {
                    Some(digit) => DIGITS[digit as usize][row],
                    None => COLON[row],
                })
                .collect()
        })
        .collect()
}

fn art_width<S: AsRef<str>>(art: &[S]) -> usize {
    art.iter().map(|line| line.as_ref().chars().count()).max().unwrap_or(0)
}

/// Largest scale fitting the space, capped at `max`. Zero means it doesn't fit at all.
fn fit_scale(art_width: usize, art_height: usize, width: usize, height: usize, max: usize) -> usize {
    (width / art_width.max(1)).min(height / art_height.max(1)).min(max)
}

/// Renders art at a whole-number scale. At scale 1 it is plain ANSI art; beyond that
/// box-drawing lines can't be enlarged, so they become a solid drop shadow.
fn scaled_art<S: AsRef<str>>(art: &[S], scale: usize, face: Color, shadow: Color) -> Vec<Line<'static>> {
    let mut lines = Vec::with_capacity(art.len() * scale);
    for row in art {
        let mut spans = Vec::new();
        let mut run = String::new();
        let mut run_color = face;
        for c in row.as_ref().chars() {
            let (glyph, color) = match c {
                '█' | ' ' => (c, face),
                _ if scale == 1 => (c, face),
                _ => ('█', shadow),
            };
            if color != run_color && !run.is_empty() {
                spans.push(Span::styled(std::mem::take(&mut run), Style::new().fg(run_color)));
            }
            run_color = color;
            run.extend(std::iter::repeat_n(glyph, scale));
        }
        spans.push(Span::styled(run, Style::new().fg(run_color)));
        let line = Line::from(spans);
        lines.extend(std::iter::repeat_n(line, scale));
    }
    lines
}

fn word_width(word: &Word) -> usize {
    word.letters.iter().map(|letter| letter[0].chars().count()).sum()
}

/// Draws a word into a grid of `rows` lines, with each letter bobbing up and down
/// along a wave of up to `wobble` rows. `rows` must leave room for that.
fn wobbling_word(word: &Word, scale: usize, rows: usize, wobble: usize, position: Duration) -> Vec<Line<'static>> {
    let mut grid = vec![vec![(' ', WHITE); word_width(word) * scale]; rows];
    let top = rows.saturating_sub(ART_HEIGHT * scale) / 2;
    let angle = position.as_secs_f32() * std::f32::consts::TAU / WOBBLE_PERIOD_SECS;
    let mut left = 0;
    for (index, letter) in word.letters.iter().enumerate() {
        let wave = (angle + index as f32 * WOBBLE_LETTER_PHASE).sin();
        let offset = (wave * wobble as f32).round() as isize;
        for (row, text) in letter.iter().enumerate() {
            for (column, c) in text.chars().enumerate() {
                let cell = match c {
                    '█' | ' ' => (c, WHITE),
                    _ if scale == 1 => (c, WHITE),
                    _ => ('█', RED_SHADOW),
                };
                for y in 0..scale {
                    for x in 0..scale {
                        let line = (top + row * scale + y) as isize + offset;
                        if let Some(target) = usize::try_from(line).ok().and_then(|line| grid.get_mut(line)) {
                            target[left + column * scale + x] = cell;
                        }
                    }
                }
            }
        }
        left += letter[0].chars().count() * scale;
    }
    grid.into_iter().map(grid_line).collect()
}

fn grid_line(cells: Vec<(char, Color)>) -> Line<'static> {
    let mut spans = Vec::new();
    let mut run = String::new();
    let mut run_color = WHITE;
    for (c, color) in cells {
        if color != run_color && !run.is_empty() {
            spans.push(Span::styled(std::mem::take(&mut run), Style::new().fg(run_color)));
        }
        run_color = color;
        run.push(c);
    }
    spans.push(Span::styled(run, Style::new().fg(run_color)));
    Line::from(spans)
}

/// The door banner. Its height depends only on the screen, never on the current
/// word, so everything below it stays still. Driven by the song position, so each
/// door song starts on the first word.
fn banner(position: Duration, width: usize, room: usize) -> Vec<Line<'static>> {
    let bold = Style::new().add_modifier(Modifier::BOLD);
    let index = (position.as_millis() / WORD_TIME_MILLIS) as usize % WORDS.len();
    let word = &WORDS[index];

    let max_scale = WORDS
        .iter()
        .map(|word| fit_scale(word_width(word), ART_HEIGHT, width, room / 2, usize::MAX))
        .max()
        .unwrap_or(0);
    if max_scale == 0 {
        return vec![text_line(fit_text(word.name, width), bold)];
    }
    let wobble = if max_scale >= 2 { 2 } else { 1 };
    let rows = ART_HEIGHT * max_scale + 2 * wobble;
    let scale = (width / word_width(word)).min(max_scale);
    if scale == 0 {
        // This word is too wide for art, though another one fits.
        let mut lines = vec![Line::default(); rows];
        lines[rows / 2] = text_line(fit_text(word.name, width), bold);
        return lines;
    }
    wobbling_word(word, scale, rows, wobble, position)
}

/// Cuts text to a display width, counting double-width characters properly.
fn fit_text(text: &str, max: usize) -> String {
    let total: usize = text.chars().map(|c| c.width().unwrap_or(0)).sum();
    if total <= max {
        return text.to_string();
    }
    let mut out = String::new();
    let mut used = 0;
    for c in text.chars() {
        let width = c.width().unwrap_or(0);
        if used + width + 1 > max {
            break;
        }
        out.push(c);
        used += width;
    }
    if max > 0 {
        out.push('…');
    }
    out
}

fn progress_bar(position: Duration, duration: Option<Duration>, width: usize) -> Option<String> {
    let total = duration?.as_secs_f64();
    if total <= 0.0 || width == 0 {
        return None;
    }
    let filled = ((position.as_secs_f64() / total).clamp(0.0, 1.0) * width as f64) as usize;
    Some("█".repeat(filled) + &"░".repeat(width - filled))
}

fn time_summary(position: Duration, duration: Option<Duration>) -> String {
    match duration {
        Some(total) => {
            let left = total.saturating_sub(position);
            format!("{} / {}   ({} left)", format_time(position), format_time(total), format_time(left))
        }
        None => format_time(position),
    }
}

fn text_line(text: String, style: Style) -> Line<'static> {
    Line::styled(text, style)
}

/// `elapsed` is the time since startup, which drives the animation.
pub fn draw(frame: &mut Frame, view: &View, elapsed: Duration) {
    let area = frame.area();
    let (width, height) = (area.width as usize, area.height as usize);
    let (background, lines) = match view.mode {
        Mode::Background => (GREEN_BG, background_screen(view, width, height, elapsed)),
        Mode::Door => (RED_BG, door_screen(view, width, height)),
    };

    let mut padded = Vec::with_capacity(height.max(lines.len()));
    padded.resize(height.saturating_sub(lines.len()) / 2, Line::default());
    padded.extend(lines);
    let screen = Paragraph::new(padded)
        .alignment(Alignment::Center)
        .style(Style::new().bg(background).fg(WHITE));
    frame.render_widget(screen, area);
}

fn warning_lines(view: &View, width: usize) -> Vec<Line<'static>> {
    match view.warning {
        Some(warning) => vec![
            text_line(
                fit_text(&format!("  {warning}  "), width),
                Style::new().bg(YELLOW).fg(BLACK).add_modifier(Modifier::BOLD),
            ),
            Line::default(),
        ],
        None => Vec::new(),
    }
}

fn volume_line(view: &View, width: usize) -> Line<'static> {
    text_line(
        fit_text(&format!("VOLUME {}%   ( + / - to change )", view.volume), width),
        Style::new(),
    )
}

fn hint_line(text: &str, width: usize, key_color: Color, text_color: Color) -> Line<'static> {
    text_line(
        fit_text(&format!("  {text}  "), width),
        Style::new().bg(key_color).fg(text_color).add_modifier(Modifier::BOLD),
    )
}

fn background_screen(view: &View, width: usize, height: usize, elapsed: Duration) -> Vec<Line<'static>> {
    let bold = Style::new().add_modifier(Modifier::BOLD);
    let mut lines = warning_lines(view, width);
    lines.push(text_line(fit_text("♪  BACKGROUND MUSIC  ♪", width), Style::new().fg(GREEN_LIGHT).add_modifier(Modifier::BOLD)));
    lines.push(Line::default());
    lines.push(text_line(
        fit_text(view.title.unwrap_or("-"), width.saturating_sub(4)),
        bold,
    ));
    lines.push(Line::default());

    const RESERVED: usize = 7; // the text below the skull
    let skull_width = SKULL[0].len() * SKULL_PIXEL_WIDTH;
    let room = height.saturating_sub(lines.len() + RESERVED);
    let scale = fit_scale(skull_width, SKULL.len(), width.saturating_sub(2), room, MAX_SKULL_SCALE);
    if scale > 0 {
        lines.extend(skull_lines(scale, eye_glow(elapsed)));
        lines.push(Line::default());
    }

    lines.push(text_line(fit_text(&time_summary(view.position, view.duration), width), Style::new()));
    if let Some(bar) = progress_bar(view.position, view.duration, width.saturating_sub(4).min(60)) {
        lines.push(text_line(bar, Style::new().fg(GREEN_LIGHT)));
    }
    lines.push(volume_line(view, width));
    lines.push(Line::default());
    lines.push(Line::default());
    lines.push(hint_line("Press SPACE or click to start the DOOR SONG", width, WHITE, GREEN_BG));
    lines
}

fn door_screen(view: &View, width: usize, height: usize) -> Vec<Line<'static>> {
    let mut lines = warning_lines(view, width);

    const RESERVED: usize = 10; // the text around the art
    let room = height.saturating_sub(lines.len() + RESERVED);
    let usable_width = width.saturating_sub(2);

    let banner_lines = banner(view.position, usable_width, room);
    let banner_rows = banner_lines.len();
    lines.extend(banner_lines);
    lines.push(Line::default());
    lines.push(text_line(
        fit_text(view.title.unwrap_or("-"), width.saturating_sub(4)),
        Style::new().add_modifier(Modifier::BOLD),
    ));
    lines.push(Line::default());

    let time_text = format_time(view.position);
    let time = big_time(&time_text);
    let time_room = room.saturating_sub(banner_rows);
    let time_scale = fit_scale(art_width(&time), ART_HEIGHT, usable_width, time_room, usize::MAX);
    if time_scale > 0 {
        lines.extend(scaled_art(&time, time_scale, YELLOW, RED_SHADOW));
        lines.push(Line::default());
    }

    lines.push(text_line(fit_text(&time_summary(view.position, view.duration), width), Style::new()));
    if let Some(bar) = progress_bar(view.position, view.duration, width.saturating_sub(4).min(60)) {
        lines.push(text_line(bar, Style::new().fg(YELLOW)));
    }
    lines.push(volume_line(view, width));
    lines.push(Line::default());
    lines.push(hint_line("Press SPACE or click to go back to background music", width, BLACK, WHITE));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};

    fn render_at(view: &View, width: u16, height: u16, elapsed: Duration) -> ratatui::buffer::Buffer {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| draw(frame, view, elapsed)).unwrap();
        terminal.backend().buffer().clone()
    }

    fn render(view: &View, width: u16, height: u16) -> ratatui::buffer::Buffer {
        render_at(view, width, height, Duration::ZERO)
    }

    fn eye_color(color: Color) -> bool {
        matches!(color, Color::Indexed(index) if EYE_RAMP.contains(&index))
    }

    fn sample(mode: Mode, warning: Option<&'static str>) -> View<'static> {
        View {
            mode,
            title: Some("Very long song title 怖い怖い with wide characters and much more text"),
            position: Duration::from_secs(83),
            duration: Some(Duration::from_secs(165)),
            warning,
            volume: 65,
        }
    }

    fn text_of(buffer: &ratatui::buffer::Buffer) -> String {
        buffer.content().iter().map(|cell| cell.symbol()).collect()
    }

    #[test]
    fn art_rows_are_rectangular() {
        let letters = WORDS.iter().flat_map(|word| word.letters.iter());
        for art in letters.chain(std::iter::once(&COLON)).chain(DIGITS.iter()) {
            assert!(art.iter().all(|row| row.chars().count() == art[0].chars().count()), "{art:?}");
        }
        for digit in &DIGITS {
            assert_eq!(digit[0].chars().count(), 9);
        }
    }

    #[test]
    fn every_size_renders_without_panicking() {
        for mode in [Mode::Background, Mode::Door] {
            for warning in [None, Some("NO SOUND DEVICE")] {
                for width in (1..240).step_by(7) {
                    for height in (1..80).step_by(3) {
                        render(&sample(mode, warning), width, height);
                    }
                }
            }
        }
    }

    #[test]
    fn whole_screen_has_the_mode_color() {
        for (mode, color) in [(Mode::Background, GREEN_BG), (Mode::Door, RED_BG)] {
            let buffer = render(&sample(mode, None), 120, 40);
            for (x, y) in [(0, 0), (119, 0), (0, 39), (119, 39), (5, 20)] {
                assert_eq!(buffer[(x, y)].bg, color, "{mode:?} at {x},{y}");
            }
        }
    }

    #[test]
    fn door_screen_has_big_words_on_a_laptop_sized_terminal() {
        for millis in [0, 750, 1500] {
            let buffer = render(&door_at(millis), 190, 50);
            let text = text_of(&buffer);
            // Scale 2+ replaces the box-drawing characters with a solid shadow.
            let filled = text.matches('█').count();
            assert!(filled > 1500, "only {filled} block cells at {millis} ms");
            assert!(text.contains("SPACE"));
            assert!(text.contains("00:0"));
        }
    }

    #[test]
    fn background_screen_shows_title_time_and_hint() {
        let view = View { title: Some("Thriller"), ..sample(Mode::Background, None) };
        let text = text_of(&render(&view, 120, 40));
        assert!(text.contains("Thriller"));
        assert!(text.contains("01:23 / 02:45"));
        assert!(text.contains("Press SPACE"));
        assert!(text.contains("VOLUME 65%"));
    }

    #[test]
    fn background_screen_shows_a_skull_with_eyes_instead_of_the_timer() {
        let buffer = render(&sample(Mode::Background, None), 190, 50);
        let cells = || buffer.content().iter().filter(|cell| cell.symbol() == "█");
        assert!(cells().any(|cell| cell.fg == WHITE), "no bone");
        assert!(!text_of(&buffer).contains("██╗"), "big digits still on screen");
        // Two eyes of 3x3 pixels, at scale 3 with each pixel 6 cells wide and 3 tall.
        assert_eq!(cells().filter(|cell| eye_color(cell.fg)).count(), 2 * 9 * 6 * 3);
    }

    #[test]
    fn skull_fits_an_80x24_terminal() {
        let buffer = render(&sample(Mode::Background, None), 80, 24);
        assert!(buffer.content().iter().any(|cell| eye_color(cell.fg)));
        assert!(text_of(&buffer).contains("SPACE"));
    }

    #[test]
    fn eyes_are_hottest_in_the_center_and_brighten_with_the_glow() {
        let center = |glow| skull_cell(b'o', 4.5, 6.5, glow).unwrap();
        let corner = skull_cell(b'o', 3.0, 5.0, 1.0).unwrap();
        assert_eq!(center(1.0), Color::Indexed(226));
        assert_ne!(center(0.0), center(1.0));
        assert_ne!(corner, center(1.0));
    }

    #[test]
    fn halo_tints_nearby_bone_only_and_vanishes_when_dim() {
        let near = skull_cell(b'X', 2.5, 6.5, 1.0).unwrap();
        assert_ne!(near, WHITE, "bone beside the eye should be lit");
        assert_eq!(skull_cell(b'X', 7.5, 0.5, 1.0), Some(WHITE), "top of the skull");
        assert_eq!(skull_cell(b'X', 2.5, 6.5, 0.0), Some(WHITE), "no glow, no halo");
        assert_eq!(skull_cell(b'.', 4.5, 6.5, 1.0), None);
    }

    #[test]
    fn glow_covers_its_range_without_strobing() {
        let at = |frame: u32| eye_glow(Duration::from_millis(50) * frame);
        let values: Vec<f32> = (0..20 * 120).map(at).collect();
        assert!(values.iter().all(|glow| (0.0..=1.0).contains(glow)));
        assert!(values.iter().copied().fold(0.0, f32::max) > 0.8, "never gets bright");
        assert!(values.iter().copied().fold(1.0, f32::min) < 0.2, "never gets dim");
        let steepest = values.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f32::max);
        assert!(steepest < 0.1, "jumps {steepest} per frame");
    }

    #[test]
    fn animation_changes_the_eyes_but_nothing_else() {
        let view = sample(Mode::Background, None);
        let dim = render_at(&view, 190, 50, Duration::from_millis(2250));
        let bright = render_at(&view, 190, 50, Duration::from_millis(750));
        assert_ne!(dim, bright);
        assert_eq!(text_of(&dim), text_of(&bright), "only colors may change");
    }

    fn door_at(millis: u64) -> View<'static> {
        View { position: Duration::from_millis(millis), ..sample(Mode::Door, None) }
    }

    /// Finds a word by a piece of art only it has, on a screen where art isn't enlarged.
    fn shown_word(millis: u64) -> &'static str {
        let text = text_of(&render(&door_at(millis), 80, 24));
        let found: Vec<_> = [("SPOOKY", "██╔═══╝ "), ("SCARY", "╚██████╗"), ("SKELETONS", "████████╗")]
            .into_iter()
            .filter(|(_, piece)| text.contains(piece))
            .map(|(name, _)| name)
            .collect();
        assert_eq!(found.len(), 1, "at {millis} ms: {found:?}");
        found[0]
    }

    #[test]
    fn door_words_take_turns_starting_with_spooky() {
        assert_eq!(shown_word(0), "SPOOKY");
        assert_eq!(shown_word(749), "SPOOKY");
        assert_eq!(shown_word(750), "SCARY");
        assert_eq!(shown_word(1500), "SKELETONS");
        assert_eq!(shown_word(2250), "SPOOKY");
    }

    #[test]
    fn every_word_is_used_in_sizes_that_fit() {
        for word in &WORDS {
            for width in [80, 120, 190, 300] {
                let scale = fit_scale(word_width(word), ART_HEIGHT, width, 100, usize::MAX);
                assert!(scale >= 1 && word_width(word) * scale <= width, "{} at {width}", word.name);
            }
        }
        assert_eq!(word_width(&WORDS[0]), 51);
        assert_eq!(word_width(&WORDS[1]), 41);
        assert_eq!(word_width(&WORDS[2]), 76);
    }

    #[test]
    fn wobble_moves_letters_without_cutting_them_off() {
        let filled = |lines: &[Line]| -> usize {
            lines.iter().flat_map(|line| &line.spans).map(|span| span.content.matches('█').count()).sum()
        };
        for word in &WORDS {
            let still = filled(&wobbling_word(word, 2, 16, 0, Duration::ZERO));
            let mut shapes = std::collections::HashSet::new();
            for millis in (0..800).step_by(50) {
                let lines = wobbling_word(word, 2, 16, 2, Duration::from_millis(millis));
                assert_eq!(lines.len(), 16);
                assert_eq!(filled(&lines), still, "{} clipped at {millis} ms", word.name);
                shapes.insert(format!("{lines:?}"));
            }
            assert!(shapes.len() > 3, "{} barely moves", word.name);
        }
    }

    #[test]
    fn letters_do_not_all_move_together() {
        let word = &WORDS[0];
        let lines = wobbling_word(word, 1, 8, 1, Duration::from_millis(100));
        let top_row_used = |column: usize| {
            lines.iter().position(|line| {
                line.spans.iter().flat_map(|span| span.content.chars()).nth(column).is_some_and(|c| c != ' ')
            })
        };
        // First column of each of the first and fourth letters.
        assert_ne!(top_row_used(0), top_row_used(8 + 8 + 9 + 1));
    }

    #[test]
    fn door_layout_stays_put_while_words_change() {
        let row_of = |millis, needle: &str| {
            let buffer = render(&door_at(millis), 190, 50);
            (0..50u16)
                .find(|&y| {
                    (0..190u16).map(|x| buffer[(x, y)].symbol()).collect::<String>().contains(needle)
                })
                .unwrap()
        };
        for millis in [0, 100, 300, 750, 1000, 1500, 1900] {
            assert_eq!(row_of(millis, "left)"), row_of(0, "left)"), "{millis}");
            assert_eq!(row_of(millis, "Press SPACE"), row_of(0, "Press SPACE"), "{millis}");
        }
    }

    #[test]
    fn narrow_screen_falls_back_to_plain_words() {
        for (millis, word) in [(0, "SPOOKY"), (750, "SCARY"), (1500, "SKELETONS")] {
            assert!(text_of(&render(&door_at(millis), 45, 30)).contains(word) || word == "SCARY");
            assert!(text_of(&render(&door_at(millis), 30, 12)).contains(word), "{word}");
        }
    }

    #[test]
    fn skull_rows_are_rectangular() {
        assert!(SKULL.iter().all(|row| row.len() == SKULL[0].len()));
    }

    #[test]
    fn door_screen_shows_the_volume() {
        assert!(text_of(&render(&sample(Mode::Door, None), 190, 50)).contains("VOLUME 65%"));
    }

    #[test]
    fn everything_important_survives_a_cramped_screen() {
        let text = text_of(&render(&sample(Mode::Door, None), 80, 24));
        assert!(text.contains("SPACE"), "hint lost at 80x24");
        let text = text_of(&render(&sample(Mode::Background, None), 80, 24));
        assert!(text.contains("SPACE"), "hint lost at 80x24");
    }

    #[test]
    fn warning_is_visible_on_both_screens() {
        for mode in [Mode::Background, Mode::Door] {
            let text = text_of(&render(&sample(mode, Some("NO SOUND DEVICE")), 120, 40));
            assert!(text.contains("NO SOUND DEVICE"));
        }
    }

    #[test]
    fn hours_are_shown_only_when_needed() {
        assert_eq!(format_time(Duration::from_secs(59)), "00:59");
        assert_eq!(format_time(Duration::from_secs(3725)), "1:02:05");
    }

    #[test]
    fn fit_text_respects_display_width() {
        assert_eq!(fit_text("short", 10), "short");
        assert_eq!(fit_text("怖い怖い", 5), "怖い…");
        assert_eq!(fit_text("abc", 0), "");
    }
}
