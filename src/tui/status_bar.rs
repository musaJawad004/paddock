//! Top line: the app name, how many servers are running, and where the data
//! comes from. Bottom line: key hints for the focused pane, read from the
//! keymap, or the current notice.

use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use unicode_width::UnicodeWidthStr;

use super::app::{App, Focus, NoticeKind};
use super::keys::Action;

pub fn render_header(app: &App, area: Rect, buf: &mut Buffer) {
    let theme = app.theme;
    let (running, stopping) = app.counts();
    let projects = app.snapshot.projects.len();
    let mut spans = vec![Span::styled(" ▞ paddock ", theme.accent()), Span::raw("  ")];
    if running > 0 {
        let servers = if running == 1 { "server" } else { "servers" };
        let in_projects = if projects == 1 { "project" } else { "projects" };
        spans.push(Span::styled(
            format!("● {running} {servers} in {projects} {in_projects} "),
            theme.success(),
        ));
    }
    if stopping > 0 {
        spans.push(Span::styled(
            format!(" ◐ {stopping} stopping "),
            theme.warning(),
        ));
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
        Focus::Servers => vec![
            (moves, "move"),
            (first(Action::Open), "open"),
            (first(Action::Copy), "copy URL"),
            (first(Action::Stop), "stop"),
            (first(Action::Kill), "kill"),
            (first(Action::NextPane), "ports"),
            (first(Action::Settings), "settings"),
            (first(Action::Help), "help"),
            (first(Action::Quit), "quit"),
        ],
        Focus::Ports => vec![
            (moves, "move"),
            ("enter".into(), "go to server"),
            (first(Action::Open), "open"),
            (first(Action::Stop), "stop"),
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
