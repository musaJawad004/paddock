//! Left column. Top: projects with their processes, status glyphs
//! (● running, ◐ starting or stopping, ✗ crashed or killed, ○ stopped),
//! port, CPU and memory, then dev servers running outside Paddock in
//! projects it does not have yet. Bottom: every listening port; listeners
//! Paddock did not start are yellow. The focused pane gets the accent border.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, HighlightSpacing, List, ListItem, ListState, StatefulWidget};

use super::app::{App, Focus};
use super::fit;
use super::theme::Theme;
use crate::model::{ProcessInfo, ProjectInfo, ResourceUsage};

pub const WIDTH: u16 = 38;
const NAME_WIDTH: usize = 10;

fn border(app: &App, pane: Focus) -> ratatui::style::Style {
    if app.focus == pane && app.overlay.is_none() {
        app.theme.focused_border()
    } else {
        app.theme.border()
    }
}

pub fn render_projects(app: &App, area: Rect, buf: &mut Buffer) {
    let theme = app.theme;
    let mut items = Vec::new();
    let mut selected_row = None;
    let mut index = 0;

    for (n, project) in app.snapshot.projects.iter().enumerate() {
        if n > 0 {
            items.push(ListItem::new(""));
        }
        items.push(ListItem::new(project_line(project, theme)));
        for process in &project.processes {
            if index == app.selected {
                selected_row = Some(items.len());
            }
            items.push(ListItem::new(process_line(process, theme)));
            index += 1;
        }
    }

    if !app.snapshot.discovered.is_empty() {
        if !items.is_empty() {
            items.push(ListItem::new(""));
        }
        items.push(ListItem::new(Line::styled(
            " Running, not added",
            theme.warning(),
        )));
        for found in &app.snapshot.discovered {
            if index == app.selected {
                selected_row = Some(items.len());
            }
            let ports: Vec<String> = found.ports.iter().map(|p| format!(":{p}")).collect();
            items.push(ListItem::new(Line::from(vec![
                Span::raw("  "),
                Span::styled("◇", theme.warning()),
                Span::raw(" "),
                Span::styled(fit(&found.name, NAME_WIDTH), theme.text()),
                Span::styled(fit(&ports.join(" "), 13), theme.dim()),
                Span::styled(found.kind.clone(), theme.dim()),
            ])));
            index += 1;
        }
    }

    let block = Block::bordered()
        .title(Span::styled(" Projects ", theme.title()))
        .border_style(border(app, Focus::Processes));
    let list = List::new(items)
        .block(block)
        .highlight_style(theme.selected())
        .highlight_spacing(HighlightSpacing::Never);
    let mut state = ListState::default().with_selected(selected_row);
    StatefulWidget::render(list, area, buf, &mut state);
}

fn project_line(project: &ProjectInfo, theme: Theme) -> Line<'static> {
    let up = project.processes.iter().filter(|p| p.state.is_up()).count();
    Line::from(vec![
        Span::styled(format!(" {}", project.name), theme.title()),
        Span::styled(
            format!("  {up}/{} up", project.processes.len()),
            theme.dim(),
        ),
    ])
}

fn process_line(process: &ProcessInfo, theme: Theme) -> Line<'static> {
    let state = theme.state(process.state);
    let port = process.port.map(|p| format!(":{p}")).unwrap_or_default();
    let usage = process.usage.map(format_usage).unwrap_or_default();
    Line::from(vec![
        Span::raw("  "),
        Span::styled(Theme::glyph(process.state), state),
        Span::raw(" "),
        Span::styled(fit(&process.id.name, NAME_WIDTH), state),
        Span::styled(fit(&port, 7), theme.dim()),
        Span::styled(usage, theme.dim()),
    ])
}

fn format_usage(usage: ResourceUsage) -> String {
    const MIB: u64 = 1024 * 1024;
    let memory = if usage.memory_bytes >= 1024 * MIB {
        format!("{:.1}G", usage.memory_bytes as f64 / (1024 * MIB) as f64)
    } else {
        format!("{}M", usage.memory_bytes / MIB)
    };
    format!("{:>4.0}% {memory:>5}", usage.cpu_percent)
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
            let line = match &port.owner {
                Some(owner) => Line::from(vec![
                    number,
                    Span::styled(fit(&owner.to_string(), rest), theme.text()),
                ]),
                None => Line::from(vec![
                    number,
                    Span::styled(
                        fit(&format!("{} pid {}", port.command, port.pid), rest),
                        theme.warning(),
                    ),
                ]),
            };
            ListItem::new(line)
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
