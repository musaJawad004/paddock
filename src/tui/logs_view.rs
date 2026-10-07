//! Main pane, logs view: output of the selected process, with its own
//! colours (see `ansi`). Follows new lines unless the user scrolled up.
//!
//! With the pane focused (Tab or Enter), a cursor picks a line; `mark`
//! starts a selection and `copy` copies it. A scrollbar on the right shows
//! where the view sits in the scrollback.
//!
//! Still to come: search, and attach mode for interactive prompts.

use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Margin, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, StatefulWidget, Widget,
};

use super::app::{App, Focus};
use super::keys::Action;
use super::theme::Theme;
use super::{ansi, brand};

pub fn render(app: &App, area: Rect, buf: &mut Buffer) {
    let theme = app.theme;
    let focused = app.focus == Focus::Logs && app.overlay.is_none();
    let border = if focused {
        theme.focused_border()
    } else {
        theme.border()
    };
    let Some(process) = app.selected_process() else {
        render_welcome(app, Block::bordered().border_style(border), area, buf);
        return;
    };

    let mut title = vec![
        Span::styled(
            format!(" {} › {} ", process.id.project, process.id.name),
            theme.title(),
        ),
        Span::styled(
            format!("{} {} ", Theme::glyph(process.state), process.state.label()),
            theme.state(process.state),
        ),
    ];
    if let Some(port) = process.port {
        title.push(Span::styled(format!(":{port} "), theme.accent()));
    }

    let empty = Default::default();
    let logs = app.selected_logs().unwrap_or(&empty);
    let height = area.height.saturating_sub(2) as usize;
    app.log_height.set(height);
    let scroll = app.scroll.min(logs.len().saturating_sub(height));
    let end = logs.len() - scroll;
    let start = end.saturating_sub(height);

    let status = if let Some((from, to)) = app.selection().filter(|_| app.mark.is_some()) {
        Line::styled(
            format!(
                " {} lines selected · {} copy ",
                to - from + 1,
                app.keymap.first(Action::Copy)
            ),
            theme.accent(),
        )
    } else if focused {
        Line::styled(
            format!(
                " {} select · {} copy · {} copy all ",
                app.keymap.first(Action::Mark),
                app.keymap.first(Action::Copy),
                app.keymap.first(Action::CopyAll)
            ),
            theme.dim(),
        )
    } else if scroll > 0 {
        Line::styled(
            format!(
                " {scroll} lines above newest · {} follow ",
                app.keymap.first(Action::Follow)
            ),
            theme.warning(),
        )
    } else {
        Line::styled(" following ", theme.dim())
    };
    let block = Block::bordered()
        .title(Line::from(title))
        .title_bottom(status.alignment(Alignment::Right))
        .border_style(border);

    if logs.is_empty() {
        let hint = if process.state.is_up() {
            "Waiting for output...".to_owned()
        } else {
            format!(
                "No output. Press {} to start.",
                app.keymap.first(Action::Start)
            )
        };
        Paragraph::new(hint)
            .style(theme.dim())
            .block(block)
            .render(area, buf);
        return;
    }

    let selection = app.selection();
    let lines: Vec<Line> = logs
        .range(start..end)
        .enumerate()
        .map(|(offset, raw)| {
            let index = start + offset;
            let (mut line, coloured) = ansi::to_line(raw);
            if !coloured {
                line = line.style(guess_style(raw, theme));
            }
            match selection {
                Some(_) if app.cursor == Some(index) => line.style(theme.selected()),
                Some((from, to)) if app.mark.is_some() && (from..=to).contains(&index) => {
                    line.style(theme.marked())
                }
                _ => line,
            }
        })
        .collect();
    Paragraph::new(lines).block(block).render(area, buf);

    if logs.len() > height {
        let mut state = ScrollbarState::new(logs.len().saturating_sub(height)).position(start);
        Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .track_symbol(Some("│"))
            .thumb_symbol("┃")
            .style(theme.border())
            .thumb_style(theme.accent())
            .render(area.inner(Margin::new(0, 1)), buf, &mut state);
    }
}

/// Output without colours of its own: highlight what looks like errors and
/// warnings.
fn guess_style(raw: &str, theme: Theme) -> Style {
    let lower = raw.to_ascii_lowercase();
    if lower.contains("error") || lower.contains("panicked") || lower.contains("err!") {
        theme.error()
    } else if lower.contains("warn") || raw.contains('⚠') {
        theme.warning()
    } else {
        Style::new()
    }
}

/// A dev server Paddock found running outside it, with the way to add it.
pub fn render_discovered(
    app: &App,
    found: &crate::model::Discovered,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = app.theme;
    let add = app.keymap.first(Action::AddProject);
    let kill = app.keymap.first(Action::Kill);
    let ports: Vec<String> = found.ports.iter().map(|p| format!(":{p}")).collect();
    let row = |label: &str, value: String| {
        Line::from(vec![
            Span::styled(format!(" {label:<10}"), theme.dim()),
            Span::styled(value, theme.text()),
        ])
    };
    let lines = vec![
        Line::styled(" RUNNING OUTSIDE PADDOCK", theme.warning()),
        Line::raw(""),
        row("Project", found.name.clone()),
        row("Folder", super::tilde(&found.path)),
        row("Kind", found.kind.clone()),
        row("Ports", ports.join("  ")),
        row("Process", format!("{} (pid {})", found.command, found.pid)),
        Line::raw(""),
        Line::styled(
            " Started from another terminal or app. Paddock cannot show its",
            theme.dim(),
        ),
        Line::styled(
            " output, but it can add the project and run it from here.",
            theme.dim(),
        ),
        Line::raw(""),
        Line::from(vec![
            Span::styled(format!(" {add}"), theme.key()),
            Span::styled(" add this project   ", theme.dim()),
            Span::styled(kill, theme.key()),
            Span::styled(" stop this server", theme.dim()),
        ]),
    ];
    let block = Block::bordered()
        .title(Span::styled(format!(" {} ", found.name), theme.title()))
        .border_style(theme.border());
    Paragraph::new(lines).block(block).render(area, buf);
}

/// Shown until the first project is added.
fn render_welcome(app: &App, block: Block, area: Rect, buf: &mut Buffer) {
    let theme = app.theme;
    let add = app.keymap.first(Action::AddProject);
    let mut lines = vec![Line::raw("")];
    lines.extend(brand::paddy(theme, false).into_iter().map(|l| l.centered()));
    lines.extend([
        Line::raw(""),
        Line::styled("No projects yet", theme.title()).centered(),
        Line::raw(""),
        Line::from(vec![
            Span::styled("Press ", theme.dim()),
            Span::styled(add, theme.key()),
            Span::styled(" to add a project folder,", theme.dim()),
        ])
        .centered(),
        Line::from(vec![
            Span::styled("or run ", theme.dim()),
            Span::styled("paddock add <folder>", theme.accent()),
            Span::styled(" in a terminal.", theme.dim()),
        ])
        .centered(),
        Line::raw(""),
        Line::styled(
            "Paddock finds what to run in package.json, Cargo.toml,",
            theme.dim(),
        )
        .centered(),
        Line::styled("compose.yaml, a Procfile or paddock.toml.", theme.dim()).centered(),
    ]);
    Paragraph::new(lines).block(block).render(area, buf);
}
