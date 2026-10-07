//! The `?` popup: every action and its keys, read from the live keymap so
//! rebinding shows up here at once. Two columns, so it fits in 80x24.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Widget};

use super::app::App;
use super::fit;
use super::keys::Action;
use super::overlay::centered;

const KEY_WIDTH: usize = 10;
const DESCRIPTION_WIDTH: usize = 25;

pub fn render(app: &App, area: Rect, buf: &mut Buffer) {
    let theme = app.theme;
    let mut entries: Vec<(String, &str)> = Action::ALL
        .iter()
        .map(|a| (app.keymap.describe(*a), a.description()))
        .collect();
    entries.push(("enter".into(), "go to server / confirm"));
    entries.push(("esc".into(), "back / cancel"));
    entries.push(("ctrl+c".into(), "quit, always"));

    let entry = |(keys, description): &(String, &str)| {
        vec![
            Span::styled(format!(" {}", fit(keys, KEY_WIDTH)), theme.key()),
            Span::styled(fit(description, DESCRIPTION_WIDTH), theme.text()),
        ]
    };

    let mut lines = vec![
        Line::from(vec![
            Span::styled(" paddock ", theme.accent()),
            Span::styled(format!("v{}  ", env!("CARGO_PKG_VERSION")), theme.dim()),
            Span::styled(
                "Every dev server on this machine, in one place",
                theme.dim(),
            ),
        ]),
        Line::raw(""),
    ];
    let half = entries.len().div_ceil(2);
    for i in 0..half {
        let mut spans = entry(&entries[i]);
        if let Some(right) = entries.get(i + half) {
            spans.push(Span::raw("  "));
            spans.extend(entry(right));
        }
        lines.push(Line::from(spans));
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        format!(
            " Data: {}. Logs show for servers started with paddock run.",
            app.source
        ),
        theme.dim(),
    ));
    lines.push(Line::styled(
        format!(
            " Change keys and theme in settings ({}). Any key closes this.",
            app.keymap.first(Action::Settings)
        ),
        theme.dim(),
    ));

    let width = (2 * (KEY_WIDTH + DESCRIPTION_WIDTH + 1) + 2 + 2) as u16;
    let popup = centered(area, width, lines.len() as u16 + 2);
    Clear.render(popup, buf);
    Paragraph::new(lines)
        .block(
            Block::bordered()
                .title(Span::styled(" Keys ", theme.title()))
                .border_style(theme.focused_border())
                .style(theme.base()),
        )
        .render(popup, buf);
}
