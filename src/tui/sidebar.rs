//! Left column. Top: every project with a dev server running, and its
//! servers with status (● running, ◐ stopping), ports, CPU and memory.
//! Bottom: the ports those servers listen on. The focused pane gets the
//! accent border.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, HighlightSpacing, List, ListItem, ListState, StatefulWidget};

use super::app::{App, Focus};
use super::fit;
use super::theme::Theme;
use crate::model::{Project, ResourceUsage, Server, ServerState};

pub const WIDTH: u16 = 38;
const NAME_WIDTH: usize = 11;
const PORT_WIDTH: usize = 8;

fn border(app: &App, pane: Focus) -> Style {
    if app.focus == pane && app.overlay.is_none() {
        app.theme.focused_border()
    } else {
        app.theme.border()
    }
}

pub fn render_servers(app: &App, area: Rect, buf: &mut Buffer) {
    let theme = app.theme;
    let mut items = Vec::new();
    let mut selected_row = None;
    for (n, project) in app.snapshot.projects.iter().enumerate() {
        if n > 0 {
            items.push(ListItem::new(""));
        }
        items.push(ListItem::new(project_line(project, theme)));
        for server in &project.servers {
            if app.is_selected(server.id) {
                selected_row = Some(items.len());
            }
            items.push(ListItem::new(server_line(server, theme)));
        }
    }
    let block = Block::bordered()
        .title(Span::styled(" Running ", theme.title()))
        .border_style(border(app, Focus::Servers));
    let list = List::new(items)
        .block(block)
        .highlight_style(theme.selected())
        .highlight_spacing(HighlightSpacing::Never);
    let mut state = ListState::default().with_selected(selected_row);
    StatefulWidget::render(list, area, buf, &mut state);
}

fn project_line(project: &Project, theme: Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!(" {}", project.name), theme.title()),
        Span::styled(format!("  {}", project.kind), theme.dim()),
    ])
}

fn server_line(server: &Server, theme: Theme) -> Line<'static> {
    let (glyph, style) = match server.state {
        ServerState::Running => ("●", theme.success()),
        ServerState::Stopping => ("◐", theme.warning()),
    };
    let port = server
        .ports
        .first()
        .map(|p| format!(":{p}"))
        .unwrap_or_default();
    Line::from(vec![
        Span::raw("  "),
        Span::styled(glyph, style),
        Span::raw(" "),
        Span::styled(fit(&server.name, NAME_WIDTH), style),
        Span::styled(fit(&format!(" {port}"), PORT_WIDTH), theme.accent()),
        Span::styled(format_usage(server.usage), theme.dim()),
    ])
}

pub fn format_usage(usage: ResourceUsage) -> String {
    format!(
        "{:>4.0}% {:>5}",
        usage.cpu_percent,
        format_memory(usage.memory_bytes)
    )
}

pub fn format_memory(bytes: u64) -> String {
    const MIB: u64 = 1024 * 1024;
    if bytes >= 1024 * MIB {
        format!("{:.1}G", bytes as f64 / (1024 * MIB) as f64)
    } else {
        format!("{}M", bytes / MIB)
    }
}

pub fn render_ports(app: &App, area: Rect, buf: &mut Buffer) {
    let theme = app.theme;
    let width = area.width.saturating_sub(2) as usize;
    let items: Vec<ListItem> = app
        .snapshot
        .ports
        .iter()
        .map(|port| {
            let number = Span::styled(fit(&format!(" :{}", port.port), 8), theme.accent());
            let rest = width.saturating_sub(8);
            let owner = port.owner;
            let name = app
                .snapshot
                .projects
                .iter()
                .find_map(|p| {
                    let s = p.servers.iter().find(|s| s.id == owner)?;
                    Some(format!("{}/{}", p.name, s.name))
                })
                .unwrap_or_else(|| port.command.clone());
            let label = Span::styled(fit(&name, rest), theme.text());
            ListItem::new(Line::from(vec![number, label]))
        })
        .collect();
    let block = Block::bordered()
        .title(Span::styled(" Ports ", theme.title()))
        .border_style(border(app, Focus::Ports));
    let selected = (app.focus == Focus::Ports).then_some(app.selected_port);
    let list = List::new(items)
        .block(block)
        .highlight_style(theme.selected())
        .highlight_spacing(HighlightSpacing::Never);
    let mut state = ListState::default().with_selected(selected);
    StatefulWidget::render(list, area, buf, &mut state);
}
