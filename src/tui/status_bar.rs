//! Top line: the app name, process counts by state, and where the data comes
//! from. Bottom line: key hints for the focused pane, read from the keymap,
//! or the current notice.

use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use unicode_width::UnicodeWidthStr;

use super::app::{App, Focus, NoticeKind};
use super::keys::Action;
use super::theme::Theme;
use crate::model::ProcessState;

pub fn render_header(app: &App, area: Rect, buf: &mut Buffer) {
    let theme = app.theme;
    let (mut running, mut busy, mut crashed, mut stopped) = (0, 0, 0, 0);
    for process in app.processes() {
        match process.state {
            ProcessState::Running => running += 1,
            ProcessState::Starting | ProcessState::Stopping => busy += 1,
            ProcessState::Crashed(_) => crashed += 1,
            ProcessState::Stopped | ProcessState::Exited => stopped += 1,
        }
    }

    let mut spans = vec![Span::styled(" ▞ paddock ", theme.accent()), Span::raw(" ")];
    let counts = [
        (running, ProcessState::Running, "running"),
        (busy, ProcessState::Starting, "busy"),
        (crashed, ProcessState::Crashed(None), "down"),
        (stopped, ProcessState::Stopped, "stopped"),
    ];
    for (count, state, label) in counts {
        if count > 0 {
            spans.push(Span::styled(
                format!(" {} {count} {label} ", Theme::glyph(state)),
                theme.state(state),
            ));
        }
    }
    Paragraph::new(Line::from(spans)).render(area, buf);
    Paragraph::new(Span::styled(format!("{} ", app.source), theme.dim()))
        .alignment(Alignment::Right)
        .render(area, buf);
}

pub fn render_footer(app: &App, area: Rect, buf: &mut Buffer) {
    let theme = app.theme;
    if let Some(notice) = &app.notice {
        let style = match notice.kind {
            NoticeKind::Info => theme.info(),
            NoticeKind::Success => theme.success(),
            NoticeKind::Error => theme.error(),
        };
        Paragraph::new(Span::styled(format!(" {}", notice.text), style)).render(area, buf);
        return;
    }

    let keys = &app.keymap;
    let first = |action| keys.first(action);
    let moves = format!("{}{}", first(Action::Up), first(Action::Down));
    let hints: Vec<(String, &str)> = match app.focus {
        Focus::Processes => vec![
            (moves, "move"),
            (first(Action::Start), "start"),
            (first(Action::Stop), "stop"),
            (first(Action::Restart), "restart"),
            (first(Action::Details), "details"),
            (first(Action::NextPane), "panes"),
            (first(Action::Settings), "settings"),
            (first(Action::Help), "help"),
            (first(Action::Quit), "quit"),
        ],
        Focus::Ports => vec![
            (moves, "move"),
            ("enter".into(), "go to process"),
            (first(Action::Kill), "kill"),
            (first(Action::NextPane), "panes"),
            ("esc".into(), "back"),
        ],
        Focus::Logs => vec![
            (moves, "line"),
            (first(Action::Mark), "select"),
            (first(Action::Copy), "copy"),
            (first(Action::CopyAll), "copy all"),
            (first(Action::Follow), "newest"),
            ("esc".into(), "back"),
        ],
    };

    let mut spans = vec![Span::raw(" ")];
    let mut used = 1;
    for (key, label) in hints.into_iter().filter(|(key, _)| !key.is_empty()) {
        let width = key.width() + label.width() + 4;
        if used + width > area.width as usize {
            break;
        }
        used += width;
        spans.push(Span::styled(key, theme.key()));
        spans.push(Span::styled(format!(" {label}   "), theme.dim()));
    }
    Paragraph::new(Line::from(spans)).render(area, buf);
}
