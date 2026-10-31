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
const EYE_RED: Color = Color::Indexed(196);

const ART_HEIGHT: usize = 6;

const SPOOKY: [&str; ART_HEIGHT] = [
    "███████╗██████╗  ██████╗  ██████╗ ██╗  ██╗██╗   ██╗",
    "██╔════╝██╔══██╗██╔═══██╗██╔═══██╗██║ ██╔╝╚██╗ ██╔╝",
    "███████╗██████╔╝██║   ██║██║   ██║█████╔╝  ╚████╔╝ ",
    "╚════██║██╔═══╝ ██║   ██║██║   ██║██╔═██╗   ╚██╔╝  ",
    "███████║██║     ╚██████╔╝╚██████╔╝██║  ██╗   ██║   ",
    "╚══════╝╚═╝      ╚═════╝  ╚═════╝ ╚═╝  ╚═╝   ╚═╝   ",
];

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

/// One character per pixel: X is bone, o is a glowing eye. A pixel is two cells wide,
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

fn skull_art() -> Vec<String> {
    SKULL
        .iter()
        .map(|row| {
            row.chars()
                .map(|pixel| match pixel {
                    'X' => "██",
                    'o' => "▓▓",
                    _ => "  ",
                })
                .collect()
        })
        .collect()
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
/// box-drawing lines can't be enlarged, so they become a solid drop shadow. `▓` always
/// means a solid block in the accent color.
fn scaled_art<S: AsRef<str>>(art: &[S], scale: usize, face: Color, shadow: Color) -> Vec<Line<'static>> {
    let mut lines = Vec::with_capacity(art.len() * scale);
    for row in art {
        let mut spans = Vec::new();
        let mut run = String::new();
        let mut run_color = face;
        for c in row.as_ref().chars() {
            let (glyph, color) = match c {
                '█' | ' ' => (c, face),
                '▓' => ('█', shadow),
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

pub fn draw(frame: &mut Frame, view: &View) {
    let area = frame.area();
    let (width, height) = (area.width as usize, area.height as usize);
    let (background, lines) = match view.mode {
        Mode::Background => (GREEN_BG, background_screen(view, width, height)),
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

fn background_screen(view: &View, width: usize, height: usize) -> Vec<Line<'static>> {
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
    let skull = skull_art();
    let room = height.saturating_sub(lines.len() + RESERVED);
    let scale = fit_scale(art_width(&skull), skull.len(), width.saturating_sub(2), room, MAX_SKULL_SCALE);
    if scale > 0 {
        lines.extend(scaled_art(&skull, scale, WHITE, EYE_RED));
        lines.push(Line::default());
    }

    lines.push(text_line(fit_text(&time_summary(view.position, view.duration), width), Style::new()));
    if let Some(bar) = progress_bar(view.position, view.duration, width.saturating_sub(4).min(60)) {
        lines.push(text_line(bar, Style::new().fg(GREEN_LIGHT)));
    }
    lines.push(volume_line(view, width));
    lines.push(Line::default());
    lines.push(Line::default());
    lines.push(hint_line("Press SPACE to start the DOOR SONG", width, WHITE, GREEN_BG));
    lines
}

fn door_screen(view: &View, width: usize, height: usize) -> Vec<Line<'static>> {
    let mut lines = warning_lines(view, width);

    const RESERVED: usize = 10; // the text around the art
    let room = height.saturating_sub(lines.len() + RESERVED);
    let usable_width = width.saturating_sub(2);

    let banner_scale = fit_scale(art_width(&SPOOKY), ART_HEIGHT, usable_width, room / 2, usize::MAX);
    if banner_scale > 0 {
        lines.extend(scaled_art(&SPOOKY, banner_scale, WHITE, RED_SHADOW));
    } else {
        lines.push(text_line(
            fit_text("SPOOKY", width),
            Style::new().add_modifier(Modifier::BOLD),
        ));
    }
    lines.push(Line::default());
    lines.push(text_line(
        fit_text(view.title.unwrap_or("-"), width.saturating_sub(4)),
        Style::new().add_modifier(Modifier::BOLD),
    ));
    lines.push(Line::default());

    let time_text = format_time(view.position);
    let time = big_time(&time_text);
    let time_room = room.saturating_sub(banner_scale * ART_HEIGHT);
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
    lines.push(hint_line("Press SPACE to go back to background music", width, BLACK, WHITE));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};

    fn render(view: &View, width: u16, height: u16) -> ratatui::buffer::Buffer {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| draw(frame, view)).unwrap();
        terminal.backend().buffer().clone()
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
        for art in std::iter::once(&SPOOKY).chain(std::iter::once(&COLON)).chain(DIGITS.iter()) {
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
    fn door_screen_has_big_spooky_on_a_laptop_sized_terminal() {
        let buffer = render(&sample(Mode::Door, None), 190, 50);
        let text = text_of(&buffer);
        // Scale 2+ replaces the box-drawing characters with a solid shadow.
        let filled = text.matches('█').count();
        assert!(filled > 1500, "only {filled} block cells");
        assert!(text.contains("SPACE"));
        assert!(text.contains("01:23"));
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
    fn background_screen_shows_a_red_eyed_skull_instead_of_the_timer() {
        let buffer = render(&sample(Mode::Background, None), 190, 50);
        let cells = || buffer.content().iter().filter(|cell| cell.symbol() == "█");
        assert!(cells().any(|cell| cell.fg == EYE_RED), "no red eyes");
        assert!(cells().any(|cell| cell.fg == WHITE), "no bone");
        // The scaled skull is 90 cells wide; the big timer's digits are gone.
        let text = text_of(&buffer);
        assert!(!text.contains("██╗"), "big digits still on screen");
        assert_eq!(cells().filter(|cell| cell.fg == EYE_RED).count(), 2 * 3 * 3 * 6 * 3);
    }

    #[test]
    fn skull_fits_an_80x24_terminal() {
        let buffer = render(&sample(Mode::Background, None), 80, 24);
        assert!(buffer.content().iter().any(|cell| cell.fg == EYE_RED));
        assert!(text_of(&buffer).contains("SPACE"));
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
