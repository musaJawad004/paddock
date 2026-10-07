//! Left column. Top: projects with their processes, status glyphs
//! (● running, ◐ starting or stopping, ✗ crashed, ○ stopped), port, CPU and
//! memory. Bottom: every listening port, with listeners Paddock did not start
//! marked as foreign.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, HighlightSpacing, List, ListItem, ListState, Paragraph, StatefulWidget, Widget,
};

use super::app::App;
use super::fit;
use super::theme::Theme;
use crate::model::{ProcessInfo, ProjectInfo, ResourceUsage};

pub const WIDTH: u16 = 42;
const NAME_WIDTH: usize = 10;

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

    let block = Block::bordered()
        .title(Span::styled(" Projects ", theme.title()))
        .border_style(theme.focused_border());
    let list = List::new(items)
        .block(block)
        .highlight_style(theme.selected())
        .highlight_spacing(HighlightSpacing::Never);
    let mut state = ListState::default().with_selected(selected_row);
    StatefulWidget::render(list, area, buf, &mut state);
}

fn project_line(project: &ProjectInfo, theme: Theme) -> Line<'static> {
    let up = project.processes.iter().filter(|p| p.state.is_up()).count();
    let summary = format!("{up}/{} up", project.processes.len());
    Line::from(vec![
        Span::styled(format!(" {}", project.name), theme.title()),
        Span::styled(format!("  {summary}"), theme.dim()),
    ])
}

fn process_line(process: &ProcessInfo, theme: Theme) -> Line<'static> {
    let state = theme.state(process.state);
    let port = process.port.map(|p| format!(":{p}")).unwrap_or_default();
    let usage = process.usage.map(format_usage).unwrap_or_default();
    Line::from(vec![
        Span::raw("   "),
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
    format!("{:>5.1}% {memory:>5}", usage.cpu_percent)
}

pub fn render_ports(app: &App, area: Rect, buf: &mut Buffer) {
    let theme = app.theme;
    let lines: Vec<Line> = app
        .snapshot
        .ports
        .iter()
        .map(|port| {
            let number = Span::styled(fit(&format!(" :{}", port.port), 8), theme.accent());
            match &port.owner {
                Some(owner) => Line::from(vec![number, Span::raw(owner.to_string())]),
                None => Line::from(vec![
                    number,
                    Span::styled(fit(&port.command, 13), theme.warning()),
                    Span::styled(format!(" pid {} · not ours", port.pid), theme.dim()),
                ]),
            }
        })
        .collect();
    let block = Block::bordered()
        .title(Span::styled(" Ports ", theme.title()))
        .border_style(theme.border());
    Paragraph::new(lines).block(block).render(area, buf);
}
