//! Main pane, details view (`i`): everything known about the selected
//! process and its project. Where it runs, since when, with which pid and
//! port, where the command came from, and what it costs.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Widget, Wrap};

use super::app::App;
use super::theme::Theme;
use super::{fit, tilde};
use crate::model::{self, ProcessState};

const LABEL_WIDTH: usize = 11;

pub fn render(app: &App, area: Rect, buf: &mut Buffer) {
    let theme = app.theme;
    let block = Block::bordered()
        .title(Span::styled(" Details ", theme.title()))
        .border_style(theme.border());
    let Some(process) = app.selected_process() else {
        Paragraph::new("No projects yet.")
            .style(theme.dim())
            .block(block)
            .render(area, buf);
        return;
    };
    let Some(project) = app
        .snapshot
        .projects
        .iter()
        .find(|p| p.name == process.id.project)
    else {
        return;
    };

    let row = |label: &str, value: Span<'static>| {
        Line::from(vec![
            Span::styled(format!(" {}", fit(label, LABEL_WIDTH)), theme.dim()),
            value,
        ])
    };
    let text = |value: String| Span::styled(value, theme.text());

    let up = project.processes.iter().filter(|p| p.state.is_up()).count();
    let ports: Vec<String> = app
        .snapshot
        .ports
        .iter()
        .filter(|p| p.owner.as_ref().is_some_and(|o| o.project == project.name))
        .map(|p| format!(":{}", p.port))
        .collect();

    let mut lines = vec![
        Line::styled(" PROJECT", theme.accent()),
        row("Name", text(project.name.clone())),
        row("Folder", text(tilde(&project.path))),
        row(
            "Processes",
            text(format!("{} ({up} running)", project.processes.len())),
        ),
        row(
            "Ports",
            text(if ports.is_empty() {
                "none open".into()
            } else {
                ports.join("  ")
            }),
        ),
        Line::raw(""),
        Line::styled(" PROCESS", theme.accent()),
        row("Name", text(process.id.name.clone())),
        row(
            "State",
            state_span(app, process.state, process.started_at_ms),
        ),
        row(
            "PID",
            text(process.pid.map_or("not running".into(), |p| p.to_string())),
        ),
    ];

    let port = match process.port {
        Some(port) if process.state == ProcessState::Running => {
            Span::styled(format!(":{port}   http://localhost:{port}"), theme.info())
        }
        Some(port) => text(format!(":{port} (when running)")),
        None => text("none".into()),
    };
    lines.push(row("Port", port));
    lines.push(row("Command", text(process.command.clone())));
    lines.push(row("Runs in", text(tilde(&process.cwd))));
    lines.push(row("From", text(process.source.clone())));
    lines.push(row("Restarts", text(process.restarts.to_string())));
    if let Some(usage) = process.usage {
        lines.push(row("CPU", text(format!("{:.1}%", usage.cpu_percent))));
        lines.push(row(
            "Memory",
            text(format!("{} MB", usage.memory_bytes / (1024 * 1024))),
        ));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled(" ", theme.dim()),
        Span::styled(app.keymap.first(super::keys::Action::Details), theme.key()),
        Span::styled(" back to logs", theme.dim()),
    ]));

    Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(block)
        .render(area, buf);
}

fn state_span(app: &App, state: ProcessState, started_at_ms: Option<u64>) -> Span<'static> {
    let mut label = format!("{} {}", Theme::glyph(state), state.label());
    if state == ProcessState::Running
        && let Some(started) = started_at_ms
    {
        let seconds = model::now_ms().saturating_sub(started) / 1000;
        label.push_str(&format!(" for {}", duration(seconds)));
    }
    Span::styled(label, app.theme.state(state))
}

fn duration(seconds: u64) -> String {
    match seconds {
        0..60 => format!("{seconds}s"),
        60..3600 => format!("{}m {}s", seconds / 60, seconds % 60),
        _ => format!("{}h {}m", seconds / 3600, seconds % 3600 / 60),
    }
}

#[cfg(test)]
mod tests {
    use super::duration;

    #[test]
    fn durations_read_naturally() {
        assert_eq!(duration(5), "5s");
        assert_eq!(duration(192), "3m 12s");
        assert_eq!(duration(7380), "2h 3m");
    }
}
