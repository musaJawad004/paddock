//! The start-up splash: Paddy trots in, the logo wipes in behind a fence,
//! then the tagline. About two seconds; any key skips it, and
//! `ui.splash = false` or `--no-splash` turns it off.
//!
//! Drawing is a pure function of the elapsed time, so it is easy to test
//! and never blocks the event loop.

use std::time::Duration;

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use super::brand::{self, LOGO, LOGO_WIDTH, PADDY_HEIGHT, PADDY_WIDTH};
use super::theme::Theme;

pub const DURATION: Duration = Duration::from_millis(2300);
const TROT: Duration = Duration::from_millis(700);
const WIPE_START: Duration = Duration::from_millis(350);
const WIPE: Duration = Duration::from_millis(700);
const TAGLINE_AT: Duration = Duration::from_millis(1150);
const BLINK_AT: Duration = Duration::from_millis(1600);
const BLINK: Duration = Duration::from_millis(140);

const TAGLINE: &str = "A terminal workspace for your dev servers";

pub fn render(theme: Theme, elapsed: Duration, area: Rect, buf: &mut Buffer) {
    let full = area.width >= LOGO_WIDTH + 4 && area.height >= 20;
    let logo_height = if full { LOGO.len() as u16 } else { 1 };
    let height = PADDY_HEIGHT + 1 + logo_height + 1 + 2 + 1;
    let width = if full { LOGO_WIDTH } else { 30.min(area.width) };

    let [column] = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .areas(area);
    let [column] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(column);
    let [paddy_row, _, logo_rows, fence_row, _, tagline_row, hint_row] = Layout::vertical([
        Constraint::Length(PADDY_HEIGHT),
        Constraint::Length(1),
        Constraint::Length(logo_height),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(column);

    render_paddy(theme, elapsed, paddy_row, buf);

    let shown = progress(elapsed, WIPE_START, WIPE);
    let reveal = |text: &str| -> String {
        let count = text.chars().count();
        let visible = (count as f32 * shown).round() as usize;
        text.chars().take(visible).collect()
    };
    let logo: Vec<Line> = if full {
        LOGO.iter()
            .map(|row| Line::styled(reveal(row), theme.accent()))
            .collect()
    } else {
        vec![Line::styled(reveal("p a d d o c k"), theme.accent()).centered()]
    };
    Paragraph::new(logo).render(logo_rows, buf);

    let fence = brand::fence(width);
    let fence_shown = reveal(&fence);
    Paragraph::new(Line::styled(fence_shown, theme.border())).render(fence_row, buf);

    if elapsed >= TAGLINE_AT {
        // Text rows may be wider than the art column on small screens.
        let full_width = |row: Rect| Rect {
            x: area.x,
            width: area.width,
            ..row
        };
        let (tagline_row, hint_row) = (full_width(tagline_row), full_width(hint_row));
        Paragraph::new(Line::styled(TAGLINE, theme.text()).centered()).render(tagline_row, buf);
        let hint = Line::from(vec![
            Span::styled(format!("v{}", env!("CARGO_PKG_VERSION")), theme.dim()),
            Span::styled("   press any key", theme.dim()),
        ])
        .centered();
        Paragraph::new(hint).render(hint_row, buf);
    }
}

/// Paddy trots in from the left edge of the column to its centre, bobbing.
fn render_paddy(theme: Theme, elapsed: Duration, row: Rect, buf: &mut Buffer) {
    let t = ease_out(progress(elapsed, Duration::ZERO, TROT));
    let centre = row.width.saturating_sub(PADDY_WIDTH) / 2;
    let x = row.x + (centre as f32 * t).round() as u16;
    let trotting = elapsed < TROT;
    let bob = trotting && (elapsed.as_millis() / 110) % 2 == 1;
    let blink = elapsed >= BLINK_AT && elapsed < BLINK_AT + BLINK;

    let area = Rect {
        x,
        y: row.y,
        width: PADDY_WIDTH.min(row.right().saturating_sub(x)),
        height: PADDY_HEIGHT,
    };
    let mut lines = brand::paddy(theme, blink);
    if bob {
        lines.insert(0, Line::raw(""));
    }
    Paragraph::new(lines).render(area, buf);
}

/// 0.0 before `start`, 1.0 after `start + length`, linear in between.
fn progress(elapsed: Duration, start: Duration, length: Duration) -> f32 {
    let into = elapsed.saturating_sub(start).as_secs_f32();
    (into / length.as_secs_f32()).clamp(0.0, 1.0)
}

fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;

    fn frame(elapsed_ms: u64, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|f| {
                render(
                    Theme::plain(),
                    Duration::from_millis(elapsed_ms),
                    f.area(),
                    f.buffer_mut(),
                )
            })
            .unwrap();
        terminal.backend().to_string()
    }

    #[test]
    fn the_last_frame_shows_logo_and_tagline() {
        insta::assert_snapshot!(frame(2000, 80, 24));
    }

    #[test]
    fn the_first_frame_has_no_logo_yet() {
        let first = frame(0, 80, 24);
        assert!(!first.contains("██████╗"));
        assert!(!first.contains(TAGLINE));
    }

    #[test]
    fn small_terminals_get_the_compact_splash() {
        insta::assert_snapshot!(frame(2000, 40, 16));
    }
}
