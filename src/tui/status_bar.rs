//! Top line: the app name, process counts by state, and where the data comes
//! from. Bottom line: key hints from `keys`, replaced by a notice when there
//! is one.

use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use super::app::App;
use super::keys::BINDINGS;
use super::theme::Theme;
use crate::model::ProcessState;

pub fn render_header(app: &App, area: Rect, buf: &mut Buffer) {
    let theme = app.theme;
    let mut running = 0;
    let mut busy = 0;
    let mut crashed = 0;
    let mut stopped = 0;
    for process in app.processes() {
        match process.state {
            ProcessState::Running => running += 1,
            ProcessState::Starting | ProcessState::Stopping => busy += 1,
            ProcessState::Crashed(_) => crashed += 1,
            ProcessState::Stopped | ProcessState::Exited => stopped += 1,
        }
    }

    let mut spans = vec![Span::styled(" paddock ", theme.accent()), Span::raw(" ")];
    let counts = [
        (running, ProcessState::Running, "running"),
        (busy, ProcessState::Starting, "busy"),
        (crashed, ProcessState::Crashed(None), "crashed"),
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
        Paragraph::new(Span::styled(format!(" {notice}"), theme.warning())).render(area, buf);
        return;
    }
    let mut spans = vec![Span::raw(" ")];
    for (key, label) in BINDINGS.iter().filter_map(|b| b.hint) {
        spans.push(Span::styled(key, theme.key()));
        spans.push(Span::styled(format!(" {label}   "), theme.dim()));
    }
    Paragraph::new(Line::from(spans)).render(area, buf);
}
