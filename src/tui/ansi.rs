//! Turns one line of raw process output into styled text.
//!
//! Dev servers colour their output with ANSI SGR codes and redraw progress
//! lines with `\r`. This keeps the colours (16, 256 and 24-bit, bold, dim,
//! italic, underline, reverse), keeps only what follows the last `\r`, and
//! drops every other escape sequence (cursor moves, clears, titles,
//! hyperlinks) so they cannot corrupt the screen.
//!
//! Full terminal emulation (`vt100`) comes with the real PTY, for programs
//! that redraw whole screens.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

const ESC: char = '\u{1b}';
const TAB_WIDTH: usize = 4;

/// Styled line, and whether the output set any colour itself.
pub fn to_line(raw: &str) -> (Line<'static>, bool) {
    let mut spans = Vec::new();
    let mut text = String::new();
    let mut style = Style::new();
    let mut coloured = false;
    let mut chars = visible_part(raw).chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            ESC => match chars.next() {
                Some('[') => {
                    let mut params = String::new();
                    let mut last = None;
                    for c in chars.by_ref() {
                        if ('\u{40}'..='\u{7e}').contains(&c) {
                            last = Some(c);
                            break;
                        }
                        params.push(c);
                    }
                    if last == Some('m') {
                        if !text.is_empty() {
                            spans.push(Span::styled(std::mem::take(&mut text), style));
                        }
                        style = apply_sgr(style, &params);
                        coloured = true;
                    }
                }
                Some(']') => skip_osc(&mut chars),
                _ => {}
            },
            '\t' => text.push_str(&" ".repeat(TAB_WIDTH - text.chars().count() % TAB_WIDTH)),
            c if c.is_control() => {}
            c => text.push(c),
        }
    }
    if !text.is_empty() {
        spans.push(Span::styled(text, style));
    }
    (Line::from(spans), coloured)
}

/// The text without any escape codes, for copying and searching.
pub fn strip(raw: &str) -> String {
    let (line, _) = to_line(raw);
    line.spans.iter().map(|s| s.content.as_ref()).collect()
}

/// A progress bar redrawn with `\r` shows only its latest state.
fn visible_part(raw: &str) -> &str {
    let trimmed = raw.trim_end_matches(['\r', '\n']);
    match trimmed.rfind('\r') {
        Some(i) => &trimmed[i + 1..],
        None => trimmed,
    }
}

fn skip_osc(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    while let Some(c) = chars.next() {
        if c == '\u{7}' || (c == ESC && chars.next_if_eq(&'\\').is_some()) {
            return;
        }
    }
}

fn apply_sgr(mut style: Style, params: &str) -> Style {
    let mut codes = params
        .split(';')
        .map(|p| p.parse::<u16>().unwrap_or(0))
        .peekable();
    if params.is_empty() {
        return Style::new();
    }
    while let Some(code) = codes.next() {
        style = match code {
            0 => Style::new(),
            1 => style.add_modifier(Modifier::BOLD),
            2 => style.add_modifier(Modifier::DIM),
            3 => style.add_modifier(Modifier::ITALIC),
            4 => style.add_modifier(Modifier::UNDERLINED),
            7 => style.add_modifier(Modifier::REVERSED),
            22 => style.remove_modifier(Modifier::BOLD | Modifier::DIM),
            23 => style.remove_modifier(Modifier::ITALIC),
            24 => style.remove_modifier(Modifier::UNDERLINED),
            27 => style.remove_modifier(Modifier::REVERSED),
            30..=37 => style.fg(basic(code - 30)),
            39 => style.fg(Color::Reset),
            40..=47 => style.bg(basic(code - 40)),
            49 => style.bg(Color::Reset),
            90..=97 => style.fg(bright(code - 90)),
            100..=107 => style.bg(bright(code - 100)),
            38 => match extended(&mut codes) {
                Some(color) => style.fg(color),
                None => style,
            },
            48 => match extended(&mut codes) {
                Some(color) => style.bg(color),
                None => style,
            },
            _ => style,
        };
    }
    style
}

/// `5;n` for the 256 palette or `2;r;g;b` for 24-bit.
fn extended(codes: &mut impl Iterator<Item = u16>) -> Option<Color> {
    match codes.next()? {
        5 => Some(Color::Indexed(codes.next()?.min(255) as u8)),
        2 => {
            let mut channel = || codes.next().map(|v| v.min(255) as u8);
            Some(Color::Rgb(channel()?, channel()?, channel()?))
        }
        _ => None,
    }
}

fn basic(n: u16) -> Color {
    match n {
        0 => Color::Black,
        1 => Color::Red,
        2 => Color::Green,
        3 => Color::Yellow,
        4 => Color::Blue,
        5 => Color::Magenta,
        6 => Color::Cyan,
        _ => Color::Gray,
    }
}

fn bright(n: u16) -> Color {
    match n {
        0 => Color::DarkGray,
        1 => Color::LightRed,
        2 => Color::LightGreen,
        3 => Color::LightYellow,
        4 => Color::LightBlue,
        5 => Color::LightMagenta,
        6 => Color::LightCyan,
        _ => Color::White,
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn plain_text_passes_through() {
        let (line, coloured) = to_line("GET / 200");
        assert_eq!(line, Line::from("GET / 200"));
        assert!(!coloured);
    }

    #[test]
    fn colours_and_resets_become_spans() {
        let (line, coloured) = to_line("\u{1b}[32m✓\u{1b}[0m Ready in \u{1b}[1m1.6s\u{1b}[22m");
        assert!(coloured);
        assert_eq!(
            line,
            Line::from(vec![
                Span::styled("✓", Style::new().fg(Color::Green)),
                Span::raw(" Ready in "),
                Span::styled("1.6s", Style::new().add_modifier(Modifier::BOLD)),
            ])
        );
    }

    #[test]
    fn extended_colours() {
        let (line, _) = to_line("\u{1b}[38;5;208mA\u{1b}[48;2;1;2;3mB");
        assert_eq!(line.spans[0].style.fg, Some(Color::Indexed(208)));
        assert_eq!(line.spans[1].style.bg, Some(Color::Rgb(1, 2, 3)));
    }

    #[test]
    fn carriage_returns_keep_the_last_redraw() {
        assert_eq!(
            strip("bundling 10%\rbundling 55%\rbundled 100%"),
            "bundled 100%"
        );
        assert_eq!(strip("done\r\n"), "done");
    }

    #[test]
    fn other_escapes_are_dropped() {
        let raw = "\u{1b}[2J\u{1b}[H\u{1b}]8;;https://x.dev\u{7}link\u{1b}]8;;\u{1b}\\ ok\u{7}";
        assert_eq!(strip(raw), "link ok");
    }

    #[test]
    fn tabs_expand_to_the_next_stop() {
        assert_eq!(strip("a\tb"), "a   b");
    }

    #[test]
    fn broken_sequences_do_not_panic() {
        for raw in [
            "\u{1b}",
            "\u{1b}[",
            "\u{1b}[38;5m",
            "\u{1b}[38;2;1m",
            "\u{1b}]no end",
        ] {
            let _ = to_line(raw);
        }
    }
}
