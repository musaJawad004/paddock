//! Main pane: everything Paddock can see about the selected server. Where
//! it runs, since when, which processes and ports, what it costs, and the
//! keys to act on it. Before the first scan it says it is looking; with no
//! servers it shows Paddy and how servers get here.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Widget, Wrap};

use super::app::App;
use super::keys::Action;
use super::sidebar::format_memory;
use super::{brand, fit, tilde};
use crate::model::{ServerState, now_ms};

const LABEL_WIDTH: usize = 10;

pub fn render(app: &App, area: Rect, buf: &mut Buffer) {
    let theme = app.theme;
    let block = Block::bordered().border_style(theme.border());
    let Some(server) = app.selected_server() else {
        render_empty(app, block, area, buf);
        return;
    };
    let project = app
        .snapshot
        .projects
        .iter()
        .find(|p| p.servers.iter().any(|s| s.id == server.id));

    let row = |label: &str, value: Span<'static>| {
        Line::from(vec![
            Span::styled(format!(" {}", fit(label, LABEL_WIDTH)), theme.dim()),
            value,
        ])
    };
    let text = |value: String| Span::styled(value, theme.text());

    let (state, state_style) = match server.state {
        ServerState::Running => {
            let seconds = (now_ms() / 1000).saturating_sub(server.id.started);
            (
                format!("● running for {}", duration(seconds)),
                theme.success(),
            )
        }
        ServerState::Stopping => ("◐ stopping".to_owned(), theme.warning()),
    };
    let urls: Vec<String> = server
        .ports
        .iter()
        .map(|p| format!("http://localhost:{p}"))
        .collect();
    let processes = if server.processes == 1 {
        "1 (just this one)".to_owned()
    } else {
        format!(
            "{} (this one and {} below it)",
            server.processes,
            server.processes - 1
        )
    };

    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                format!(" {}", project.map_or("", |p| p.name.as_str())),
                theme.accent(),
            ),
            Span::styled(" › ", theme.dim()),
            Span::styled(server.name.clone(), theme.title()),
        ]),
        Line::raw(""),
        row("State", Span::styled(state, state_style)),
        row("URL", Span::styled(urls.join("  "), theme.info())),
        row("Command", text(short_command(&server.command))),
        row("Folder", text(tilde(&server.cwd))),
    ];
    if let Some(project) = project.filter(|p| p.path != server.cwd) {
        lines.push(row(
            "Project",
            text(format!("{} ({})", tilde(&project.path), project.kind)),
        ));
    }
    lines.extend([
        row("PID", text(server.id.pid.to_string())),
        row("Processes", text(processes)),
        row("CPU", text(format!("{:.1}%", server.usage.cpu_percent))),
        row("Memory", text(format_memory(server.usage.memory_bytes))),
        Line::raw(""),
        keys_line(app),
    ]);

    Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(block.title(Span::styled(" Server ", theme.title())))
        .render(area, buf);
}

fn keys_line(app: &App) -> Line<'static> {
    let theme = app.theme;
    let mut spans = vec![Span::raw(" ")];
    for (action, label) in [
        (Action::Open, "open"),
        (Action::Copy, "copy URL"),
        (Action::CopyAll, "copy command"),
        (Action::Stop, "stop"),
        (Action::Kill, "kill"),
    ] {
        let key = app.keymap.first(action);
        if key.is_empty() {
            continue;
        }
        spans.push(Span::styled(key, theme.key()));
        spans.push(Span::styled(format!(" {label}   "), theme.dim()));
    }
    Line::from(spans)
}

fn render_empty(app: &App, block: Block, area: Rect, buf: &mut Buffer) {
    let theme = app.theme;
    let mut lines = vec![Line::raw("")];
    lines.extend(brand::paddy(theme, false).into_iter().map(|l| l.centered()));
    lines.push(Line::raw(""));
    if !app.scanned {
        lines.push(Line::styled("Looking for dev servers...", theme.dim()).centered());
    } else {
        lines.extend([
            Line::styled("No dev servers running", theme.title()).centered(),
            Line::raw(""),
            Line::styled(
                "Start one in any terminal (npm run dev, cargo run...)",
                theme.dim(),
            )
            .centered(),
            Line::styled("and it shows up here within two seconds.", theme.dim()).centered(),
        ]);
    }
    Paragraph::new(lines).block(block).render(area, buf);
}

/// Long paths shortened to their last part, so the command fits on a line:
/// `node /opt/homebrew/.../yarn.js run dev` becomes `node …/yarn.js run dev`.
/// Copying the command still copies it in full.
fn short_command(command: &str) -> String {
    command
        .split(' ')
        .map(|word| match word.rsplit_once('/') {
            Some((_, last)) if word.len() > 24 && !last.is_empty() => format!("…/{last}"),
            _ => word.to_owned(),
        })
        .collect::<Vec<_>>()
        .join(" ")
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
    use super::{duration, short_command};

    #[test]
    fn long_paths_in_commands_are_shortened() {
        assert_eq!(
            short_command("node /opt/homebrew/Cellar/yarn/1.22.22/libexec/bin/yarn.js run dev"),
            "node …/yarn.js run dev"
        );
        assert_eq!(short_command("npm run dev"), "npm run dev");
        assert_eq!(short_command("node ./server.js"), "node ./server.js");
    }

    #[test]
    fn durations_read_naturally() {
        assert_eq!(duration(5), "5s");
        assert_eq!(duration(192), "3m 12s");
        assert_eq!(duration(7380), "2h 3m");
    }
}
