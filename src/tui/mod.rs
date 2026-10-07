//! The terminal UI. Holds no process handles: everything it knows arrives as
//! `ipc::Event`, everything it does leaves as `ipc::Request`.
//!
//! Built the way `.claude/skills/ratatui-tui/SKILL.md` describes: state and
//! updates in `app`, one view module per screen area, keys in one keymap,
//! colours in one theme. Side effects (copying, opening a URL, saving the
//! config) run on blocking tasks and report back as messages, so the UI
//! never stalls.

pub mod app;
mod brand;
mod clipboard;
mod details;
mod help;
pub mod keys;
mod overlay;
pub mod palette;
mod settings;
mod sidebar;
mod splash;
mod status_bar;
pub mod theme;

use std::io::{self, Write};
use std::path::Path;
use std::process::{Command, Stdio};
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
    Copied { what: String, osc52: Option<String> },
    Opened(Result<(), String>),
    Saved(Result<(), String>),
}

async fn event_loop(
    terminal: &mut DefaultTerminal,
    mut client: ClientEnd,
    options: Options,
) -> io::Result<()> {
    let mut app = App::new(options.config, options.source, options.splash);
    if let Some(notice) = options.notice {
        app.notify(NoticeKind::Error, notice);
    }
    let mut input = EventStream::new();
    let mut frame = tokio::time::interval(FRAME);
    let (done_tx, mut done_rx) = mpsc::channel::<Done>(16);
    let mut monitor_open = true;
    // Closing the terminal window or `kill` ends the loop normally, so the
    // terminal is restored.
    let mut closed = window_closed()?;

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
            event = client.events.recv(), if monitor_open => match event {
                Some(event) => app.update(Msg::Monitor(event)),
                None => {
                    monitor_open = false;
                    app.update(Msg::MonitorGone)
                }
            },
            Some(done) = done_rx.recv() => match done {
                Done::Copied { what, osc52 } => {
                    if let Some(sequence) = osc52 {
                        let mut out = io::stdout();
                        out.write_all(sequence.as_bytes())?;
                        out.flush()?;
                    }
                    app.update(Msg::Copied(what))
                }
                Done::Opened(Ok(())) => None,
                Done::Opened(Err(err)) => {
                    app.notify(NoticeKind::Error, err);
                    None
                }
                Done::Saved(result) => app.update(Msg::Saved(result)),
            },
            _ = &mut closed => break,
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
                    app.update(Msg::MonitorGone);
                }
            }
            Some(Effect::Copy { text, what }) => spawn_done(&done_tx, move || {
                let osc52 = match clipboard::copy(&text) {
                    clipboard::Copied::System => None,
                    clipboard::Copied::Osc52(sequence) => Some(sequence),
                };
                Done::Copied { what, osc52 }
            }),
            Some(Effect::Open(url)) => spawn_done(&done_tx, move || Done::Opened(open(&url))),
            Some(Effect::Save(config)) => spawn_done(&done_tx, move || {
                Done::Saved(config::save(&config).map_err(|e| e.to_string()))
            }),
        }
    }
    Ok(())
}

/// Runs `work` on a blocking thread and sends its result back to the loop.
fn spawn_done(done: &mpsc::Sender<Done>, work: impl FnOnce() -> Done + Send + 'static) {
    let done = done.clone();
    tokio::spawn(async move {
        if let Ok(result) = tokio::task::spawn_blocking(work).await {
            let _ = done.send(result).await;
        }
    });
}

type Closed = std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>;

/// Resolves when the terminal window closes or the process is asked to quit.
#[cfg(unix)]
fn window_closed() -> io::Result<Closed> {
    use tokio::signal::unix::{SignalKind, signal};
    let mut hangup = signal(SignalKind::hangup())?;
    let mut terminate = signal(SignalKind::terminate())?;
    Ok(Box::pin(async move {
        tokio::select! {
            _ = hangup.recv() => {}
            _ = terminate.recv() => {}
        }
    }))
}

#[cfg(windows)]
fn window_closed() -> io::Result<Closed> {
    let mut close = tokio::signal::windows::ctrl_close()?;
    Ok(Box::pin(async move {
        close.recv().await;
    }))
}

/// Hands a URL to the OS. Paddock itself opens no connection.
fn open(url: &str) -> Result<(), String> {
    let (program, args): (&str, &[&str]) = if cfg!(target_os = "macos") {
        ("open", &[])
    } else if cfg!(windows) {
        // The URL is built from a port number, so it holds nothing cmd
        // could misread.
        ("cmd", &["/C", "start", ""])
    } else {
        ("xdg-open", &[])
    };
    Command::new(program)
        .args(args)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|err| format!("Could not run {program}: {err}"))
        .and_then(|status| {
            status
                .success()
                .then_some(())
                .ok_or_else(|| format!("{program} could not open {url}"))
        })
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
    match std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
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
