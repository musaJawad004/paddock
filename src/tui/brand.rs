//! Paddock's logo and mascot.
//!
//! The mascot is Paddy, a pony: a paddock is the fenced field horses live
//! in, and Paddock is where your dev servers live. Paddy is pixel art drawn
//! with half blocks, so one character cell holds two square pixels, one
//! above the other.

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use super::theme::{Theme, mascot};

/// "PADDOCK" in the ANSI Shadow figlet style, one row per string.
pub const LOGO: [&str; 6] = [
    "██████╗  █████╗ ██████╗ ██████╗  ██████╗  ██████╗██╗  ██╗",
    "██╔══██╗██╔══██╗██╔══██╗██╔══██╗██╔═══██╗██╔════╝██║ ██╔╝",
    "██████╔╝███████║██║  ██║██║  ██║██║   ██║██║     █████╔╝ ",
    "██╔═══╝ ██╔══██║██║  ██║██║  ██║██║   ██║██║     ██╔═██╗ ",
    "██║     ██║  ██║██████╔╝██████╔╝╚██████╔╝╚██████╗██║  ██╗",
    "╚═╝     ╚═╝  ╚═╝╚═════╝ ╚═════╝  ╚═════╝  ╚═════╝╚═╝  ╚═╝",
];

pub const LOGO_WIDTH: u16 = 57;

/// Paddy in profile, 16 by 12 pixels, facing right. `#` coat, `m` mane,
/// `w` blaze, `p` muzzle, `e` eye, `n` nostril, `.` empty.
const PADDY: [&str; 12] = [
    ".......#.#......",
    "......####......",
    "....mm#####.....",
    "...mmm##e###....",
    "..mmmm######w...",
    "..mmmm#######w..",
    "..mmm#########w.",
    "..mmm####..#pppp",
    "..mmm###....ppnp",
    "..mmm###.....pp.",
    "..mmm###........",
    "..mmm###........",
];

pub const PADDY_WIDTH: u16 = 16;
pub const PADDY_HEIGHT: u16 = 6;

/// Paddy as six lines of text. `blink` closes the eye for one frame.
pub fn paddy(theme: Theme, blink: bool) -> Vec<Line<'static>> {
    let pixel = |row: usize, col: usize| -> Option<Color> {
        let c = PADDY[row].as_bytes()[col];
        // A closed eye is a dark lid line.
        let c = if blink && c == b'e' { b'm' } else { c };
        match c {
            b'#' => Some(mascot::COAT),
            b'm' => Some(mascot::MANE),
            b'w' => Some(mascot::BLAZE),
            b'p' => Some(mascot::MUZZLE),
            b'e' | b'n' => theme.has_color().then_some(mascot::DARK),
            _ => None,
        }
    };

    (0..PADDY.len())
        .step_by(2)
        .map(|row| {
            let spans = (0..PADDY_WIDTH as usize)
                .map(|col| half_block(theme, pixel(row, col), pixel(row + 1, col)))
                .collect::<Vec<_>>();
            Line::from(spans)
        })
        .collect()
}

/// One cell showing a top and a bottom pixel.
fn half_block(theme: Theme, top: Option<Color>, bottom: Option<Color>) -> Span<'static> {
    let paint = |color: Color| {
        if theme.has_color() {
            Style::new().fg(theme.color(color))
        } else {
            Style::new()
        }
    };
    match (top, bottom) {
        (None, None) => Span::raw(" "),
        (Some(t), None) => Span::styled("▀", paint(t)),
        (None, Some(b)) => Span::styled("▄", paint(b)),
        (Some(t), Some(b)) if t == b || !theme.has_color() => Span::styled("█", paint(t)),
        (Some(t), Some(b)) => Span::styled("▀", paint(t).bg(theme.color(b))),
    }
}

/// A fence rail of `width` columns with a post every six.
pub fn fence(width: u16) -> String {
    (0..width)
        .map(|i| if i % 6 == 0 { '╫' } else { '─' })
        .collect()
}

#[cfg(test)]
mod tests {
    use unicode_width::UnicodeWidthStr;

    use super::*;

    #[test]
    fn logo_rows_are_all_the_same_width() {
        for row in LOGO {
            assert_eq!(row.width(), LOGO_WIDTH as usize, "{row}");
        }
    }

    #[test]
    fn paddy_is_a_regular_grid() {
        for row in PADDY {
            assert_eq!(row.len(), PADDY_WIDTH as usize);
        }
        assert_eq!(PADDY.len(), PADDY_HEIGHT as usize * 2);
        let lines = paddy(Theme::plain(), false);
        assert_eq!(lines.len(), PADDY_HEIGHT as usize);
        assert!(lines.iter().all(|l| l.width() == PADDY_WIDTH as usize));
    }
}
