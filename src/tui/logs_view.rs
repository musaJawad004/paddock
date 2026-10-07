//! Main pane: output of the selected process. Follows new lines unless the
//! user scrolled up. Lines that look like errors or warnings get the theme's
//! colours.
//!
//! Still to come: render from the process's `vt100` screen so colours and
//! cursor movement survive, search, and attach mode for interactive prompts.

use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Widget};

use super::app::App;
use super::theme::Theme;

pub fn render(app: &App, area: Rect, buf: &mut Buffer) {
    let theme = app.theme;
    let Some(process) = app.selected_process() else {
        Paragraph::new("No projects yet.")
            .style(theme.dim())
            .block(Block::bordered().border_style(theme.border()))
            .render(area, buf);
        return;
    };

    let state = theme.state(process.state);
    let mut title = vec![
        Span::styled(
            format!(" {} › {} ", process.id.project, process.id.name),
            theme.title(),
        ),
        Span::styled(
            format!("{} {} ", Theme::glyph(process.state), process.state.label()),
            state,
        ),
    ];
    if let Some(port) = process.port {
        title.push(Span::styled(format!(":{port} "), theme.accent()));
    }
    title.push(Span::styled(format!("$ {} ", process.command), theme.dim()));

    let empty = Default::default();
    let logs = app.selected_logs().unwrap_or(&empty);
    let height = area.height.saturating_sub(2) as usize;
    let scroll = app.scroll.min(logs.len().saturating_sub(height));
    let end = logs.len() - scroll;
    let start = end.saturating_sub(height);

    let position = if scroll == 0 {
        Line::styled(" following ", theme.dim())
    } else {
        Line::styled(
            format!(" {scroll} lines above newest · End to follow "),
            theme.warning(),
        )
    };
    let block = Block::bordered()
        .title(Line::from(title))
        .title_bottom(position.alignment(Alignment::Right))
        .border_style(theme.border());

    if logs.is_empty() {
        let hint = if process.state.is_up() {
            "Waiting for output..."
        } else {
            "No output. Press s to start."
        };
        Paragraph::new(hint)
            .style(theme.dim())
            .block(block)
            .render(area, buf);
        return;
    }

    let lines: Vec<Line> = logs
        .range(start..end)
        .map(|line| Line::styled(line.as_str(), line_style(line, theme)))
        .collect();
    Paragraph::new(lines).block(block).render(area, buf);
}

fn line_style(line: &str, theme: Theme) -> Style {
    let lower = line.to_ascii_lowercase();
    if lower.contains("error") || lower.contains("panicked") || lower.contains("err!") {
        theme.error()
    } else if lower.contains("warn") || line.contains('⚠') {
        theme.warning()
    } else {
        Style::new()
    }
}
