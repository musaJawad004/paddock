//! The `?` popup: every binding from the key table, centered over the UI.

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Widget};

use super::app::App;
use super::fit;
use super::keys::BINDINGS;

pub fn render(app: &App, area: Rect, buf: &mut Buffer) {
    let theme = app.theme;
    let mut lines = vec![Line::raw("")];
    for binding in BINDINGS {
        lines.push(Line::from(vec![
            Span::styled(fit(&format!("  {}", binding.keys), 12), theme.key()),
            Span::raw(binding.description),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        format!("  Data: {}.", app.source),
        theme.dim(),
    ));
    lines.push(Line::styled("  Press ? or Esc to close.", theme.dim()));

    let height = lines.len() as u16 + 2;
    let [popup] = Layout::horizontal([Constraint::Length(54)])
        .flex(Flex::Center)
        .areas(area);
    let [popup] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(popup);

    Clear.render(popup, buf);
    Paragraph::new(lines)
        .block(
            Block::bordered()
                .title(Span::styled(" Keys ", theme.title()))
                .border_style(theme.focused_border()),
        )
        .render(popup, buf);
}
