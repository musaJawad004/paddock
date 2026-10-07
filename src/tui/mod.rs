//! The terminal UI. Holds no process handles: everything it knows arrives as
//! `ipc::Event`, everything it does leaves as `ipc::Request`.
//!
//! Built the way `.claude/skills/ratatui-tui/SKILL.md` describes: state and
//! updates in `app`, one view module per screen area, keys in one table,
//! colours in one theme.

pub mod app;
mod help;
pub mod keys;
mod logs_view;
pub mod palette;
mod sidebar;
mod status_bar;
pub mod theme;

use std::io;
use std::time::Duration;

use crossterm::event::{Event as TermEvent, EventStream, KeyEventKind};
use futures::StreamExt;
use ratatui::DefaultTerminal;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::ipc::transport::ClientEnd;
use app::{App, Msg};
use theme::Theme;

/// Draw at most this often; changes in between are coalesced into one frame.
const FRAME: Duration = Duration::from_millis(33);

/// Takes over the terminal until the user quits. `source` is shown in the
/// header so it is always clear where the data comes from.
pub async fn run(client: ClientEnd, source: &str) -> io::Result<()> {
    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, client, source).await;
    ratatui::restore();
    result
}

async fn event_loop(
    terminal: &mut DefaultTerminal,
    mut client: ClientEnd,
    source: &str,
) -> io::Result<()> {
    let mut app = App::new(Theme::from_env(), source);
    let mut input = EventStream::new();
    let mut frame = tokio::time::interval(FRAME);
    let mut backend_open = true;

    while !app.should_quit() {
        let request = tokio::select! {
            event = input.next() => match event {
                Some(Ok(TermEvent::Key(key))) if key.kind == KeyEventKind::Press => {
                    app.update(Msg::Key(key))
                }
                Some(Ok(TermEvent::Resize(..))) => app.update(Msg::Resize),
                Some(Ok(_)) => None,
                Some(Err(err)) => return Err(err),
                None => break,
            },
            event = client.events.recv(), if backend_open => match event {
                Some(event) => app.update(Msg::Backend(event)),
                None => {
                    backend_open = false;
                    app.update(Msg::BackendGone)
                }
            },
            _ = frame.tick() => {
                if app.take_dirty() {
                    terminal.draw(|f| f.render_widget(&app, f.area()))?;
                }
                None
            }
        };
        if let Some(request) = request
            && client.requests.send(request).await.is_err()
        {
            app.update(Msg::BackendGone);
        }
    }
    Ok(())
}

/// Cuts or pads `text` to exactly `width` terminal columns, counting wide
/// characters as two. Cut text ends in "…".
pub(crate) fn fit(text: &str, width: usize) -> String {
    if text.width() <= width {
        return format!("{text}{}", " ".repeat(width - text.width()));
    }
    let mut out = String::new();
    let mut used = 0;
    for c in text.chars() {
        let w = c.width().unwrap_or(0);
        if used + w + 1 > width {
            break;
        }
        out.push(c);
        used += w;
    }
    out.push('…');
    format!("{out}{}", " ".repeat(width.saturating_sub(used + 1)))
}

#[cfg(test)]
mod tests {
    use super::fit;

    #[test]
    fn fit_pads_short_text() {
        assert_eq!(fit("web", 5), "web  ");
    }

    #[test]
    fn fit_cuts_long_text_with_an_ellipsis() {
        assert_eq!(fit("storefront", 6), "store…");
    }

    #[test]
    fn fit_counts_wide_characters_as_two_columns() {
        assert_eq!(fit("日本語ab", 5), "日本…");
    }
}
