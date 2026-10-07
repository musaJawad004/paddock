//! Main pane, top: the selected server at a glance. Where it runs, since
//! when, which ports, what it costs. Before the first scan it says it is
//! looking; with no servers it shows Paddy and how servers get here.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Widget};

use super::app::App;
use super::sidebar::format_memory;
use super::{brand, tilde};
use crate::model::{ServerState, now_ms};

/// Rows the server header takes, borders included.
pub const HEIGHT: u16 = 7;

pub fn render(app: &App, area: Rect, buf: &mut Buffer) {
    let theme = app.theme;
    let block = Block::bordered().border_style(theme.border());
    let Some(server) = app.selected_server() else {
        render_empty(app, block, area, buf);
        return;
    };

    let (state, state_style) = match server.state {
        ServerState::Running => {
            let seconds = (now_ms() / 1000).saturating_sub(server.id.started);
            (format!("● running {}", duration(seconds)), theme.success())
        }
        ServerState::Stopping => ("◐ stopping".to_owned(), theme.warning()),
    };
    let urls: Vec<String> = server
        .ports
        .iter()
        .map(|p| format!("http://localhost:{p}"))
        .collect();
    let label = |text: &str| Span::styled(format!(" {text:<9}"), theme.dim());
    let lines = vec![
        Line::from(vec![
            Span::styled(format!(" {} ", app.server_label(server.id)), theme.title()),
            Span::styled(state, state_style),
        ]),
        Line::from(vec![
            label("URL"),
            Span::styled(urls.join("  "), theme.info()),
        ]),
        Line::from(vec![
            label("Command"),
            Span::styled(short_command(&server.command), theme.text()),
        ]),
        Line::from(vec![
            label("Folder"),
            Span::styled(tilde(&server.cwd), theme.text()),
        ]),
        Line::from(vec![
            label("Process"),
            Span::styled(
                format!(
                    "pid {} · {} process{} · {:.1}% CPU · {}",
                    server.id.pid,
                    server.processes,
                    if server.processes == 1 { "" } else { "es" },
                    server.usage.cpu_percent,
                    format_memory(server.usage.memory_bytes)
                ),
                theme.text(),
            ),
        ]),
    ];
    Paragraph::new(lines).block(block).render(area, buf);
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
            Line::raw(""),
            Line::styled(
                "To see its logs here too, start it with paddock run,",
                theme.dim(),
            )
            .centered(),
            Line::styled(
                "or add  eval \"$(paddock init zsh)\"  to ~/.zshrc once.",
                theme.dim(),
            )
            .centered(),
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
