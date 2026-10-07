//! The terminal UI. Holds no process handles: everything it knows arrives as
//! `ipc::Event`, everything it does leaves as `ipc::Request`.
//!
//! Built the way `.claude/skills/ratatui-tui/SKILL.md` describes: state and
//! updates in `app`, one view module per screen area, keys in one keymap,
//! colours in one theme. Side effects (copying, saving the config) run on
//! blocking tasks and report back as messages, so the UI never stalls.

mod ansi;
pub mod app;
mod brand;
mod clipboard;
mod details;
mod help;
pub mod keys;
mod logs_view;
mod overlay;
pub mod palette;
mod settings;
mod sidebar;
mod splash;
mod status_bar;
pub mod theme;

use std::io::{self, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use crossterm::event::{Event as TermEvent, EventStream, KeyEventKind};
use futures::StreamExt;
use ratatui::DefaultTerminal;
use tokio::sync::mpsc;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::config::{self, Config};
use crate::ipc::transport::ClientEnd;
use app::{App, Effect, Msg, NoticeKind};

/// Draw at most this often; changes in between are coalesced into one frame.
const FRAME: Duration = Duration::from_millis(33);

pub struct Options {
    /// Shown in the header so it is always clear where the data comes from.
    pub source: String,
    pub config: Config,
    pub splash: bool,
    /// Shown once at start-up, e.g. why the config could not be read.
    pub notice: Option<String>,
    /// Quitting stops the processes, so the TUI asks before quitting.
    pub stops_on_quit: bool,
    /// Suggested when adding a project.
    pub cwd: std::path::PathBuf,
}

/// Takes over the terminal until the user quits.
pub async fn run(client: ClientEnd, options: Options) -> io::Result<()> {
    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, client, options).await;
    ratatui::restore();
    result
}

/// Results of side effects, coming back from blocking tasks.
enum Done {
    Copied { lines: usize, osc52: Option<String> },
    Saved(Result<(), String>),
}

async fn event_loop(
    terminal: &mut DefaultTerminal,
    mut client: ClientEnd,
    options: Options,
) -> io::Result<()> {
    let mut app = App::new(options.config, options.source, options.splash)
        .with_backend(options.stops_on_quit, options.cwd);
    if let Some(notice) = options.notice {
        app.notify(NoticeKind::Error, notice);
    }
    let mut input = EventStream::new();
    let mut frame = tokio::time::interval(FRAME);
    let (done_tx, mut done_rx) = mpsc::channel::<Done>(16);
    let mut backend_open = true;
    // Closing the terminal window or `kill` ends the loop normally, so the
    // terminal is restored and the processes are stopped.
    let mut hangup = unix_signal(tokio::signal::unix::SignalKind::hangup())?;
    let mut terminate = unix_signal(tokio::signal::unix::SignalKind::terminate())?;

    while !app.should_quit() {
        let effect = tokio::select! {
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
            Some(done) = done_rx.recv() => match done {
                Done::Copied { lines, osc52 } => {
                    if let Some(sequence) = osc52 {
                        let mut out = io::stdout();
                        out.write_all(sequence.as_bytes())?;
                        out.flush()?;
                    }
                    app.update(Msg::Copied { lines })
                }
                Done::Saved(result) => app.update(Msg::Saved(result)),
            },
            _ = hangup.recv() => break,
            _ = terminate.recv() => break,
            _ = frame.tick() => {
                app.update(Msg::Tick(Instant::now()));
                if app.take_dirty() {
                    terminal.draw(|f| f.render_widget(&app, f.area()))?;
                }
                None
            }
        };

        match effect {
            None => {}
            Some(Effect::Send(request)) => {
                if client.requests.send(request).await.is_err() {
                    app.update(Msg::BackendGone);
                }
            }
            Some(Effect::Copy { text, lines }) => {
                let done = done_tx.clone();
                tokio::spawn(async move {
                    let copied = tokio::task::spawn_blocking(move || clipboard::copy(&text)).await;
                    let osc52 = match copied {
                        Ok(clipboard::Copied::System) => None,
                        Ok(clipboard::Copied::Osc52(sequence)) => Some(sequence),
                        Err(_) => return,
                    };
                    let _ = done.send(Done::Copied { lines, osc52 }).await;
                });
            }
            Some(Effect::Save(config)) => {
                let done = done_tx.clone();
                tokio::spawn(async move {
                    let saved = tokio::task::spawn_blocking(move || config::save(&config)).await;
                    let result = match saved {
                        Ok(result) => result.map_err(|e| e.to_string()),
                        Err(e) => Err(e.to_string()),
                    };
                    let _ = done.send(Done::Saved(result)).await;
                });
            }
        }
    }
    Ok(())
}

fn unix_signal(kind: tokio::signal::unix::SignalKind) -> io::Result<tokio::signal::unix::Signal> {
    tokio::signal::unix::signal(kind)
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

/// A path with the home folder shown as `~`.
pub(crate) fn tilde(path: &Path) -> String {
    let text = path.display().to_string();
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() && text.starts_with(&home) => {
            format!("~{}", &text[home.len()..])
        }
        _ => text,
    }
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
