//! The folder picker that `add project` opens: browse instead of typing a
//! path.
//!
//! The first row adds the folder being shown; the rest are its subfolders,
//! each labelled with what kind of project it looks like (node, rust...) or
//! "added" if Paddock already has it. Typing filters the list; a filter
//! starting with `/` or `~` is a path that Enter jumps to.
//!
//! Listing one folder is fast enough to do on the UI thread; nothing here
//! reads file contents.

use std::fs;
use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Widget};

use super::app::{App, Effect};
use super::overlay::{Overlay, centered};
use super::{fit, tilde};
use crate::config;
use crate::detect;
use crate::ipc::protocol::Request;

pub struct FolderPicker {
    pub dir: PathBuf,
    entries: Vec<Entry>,
    pub filter: String,
    /// Row 0 is "add this folder"; rows after it are `visible()` entries.
    pub selected: usize,
    pub error: Option<String>,
}

struct Entry {
    name: String,
    path: PathBuf,
    kind: Option<&'static str>,
}

impl FolderPicker {
    pub fn open(dir: PathBuf) -> Self {
        let mut picker = Self {
            dir,
            entries: Vec::new(),
            filter: String::new(),
            selected: 0,
            error: None,
        };
        picker.load();
        picker
    }

    fn load(&mut self) {
        self.entries = fs::read_dir(&self.dir)
            .map(|read| {
                read.flatten()
                    .map(|e| e.path())
                    .filter(|p| p.is_dir())
                    .filter_map(|path| {
                        let name = path.file_name()?.to_string_lossy().into_owned();
                        Some(Entry {
                            kind: detect::project_kind(&path),
                            name,
                            path,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        // Projects first, then the rest, each alphabetically.
        self.entries.sort_by(|a, b| {
            (a.kind.is_none(), a.name.to_lowercase())
                .cmp(&(b.kind.is_none(), b.name.to_lowercase()))
        });
        self.selected = 0;
    }

    fn go(&mut self, dir: PathBuf) {
        self.dir = dir;
        self.filter.clear();
        self.error = None;
        self.load();
    }

    fn visible(&self) -> Vec<&Entry> {
        let filter = self.filter.to_lowercase();
        self.entries
            .iter()
            .filter(|e| !e.name.starts_with('.') || filter.starts_with('.'))
            .filter(|e| filter.is_empty() || e.name.to_lowercase().contains(&filter))
            .collect()
    }

    fn selected_entry(&self) -> Option<&Entry> {
        self.selected
            .checked_sub(1)
            .and_then(|i| self.visible().get(i).copied())
    }

    fn is_path_filter(&self) -> bool {
        self.filter.starts_with('/') || self.filter.starts_with('~')
    }
}

impl App {
    pub(super) fn on_folder_key(
        &mut self,
        mut picker: FolderPicker,
        key: KeyEvent,
    ) -> (Option<Overlay>, Option<Effect>) {
        let rows = 1 + picker.visible().len();
        match key.code {
            KeyCode::Esc => return (None, None),
            KeyCode::Up => picker.selected = picker.selected.saturating_sub(1),
            KeyCode::Down => picker.selected = (picker.selected + 1).min(rows - 1),
            KeyCode::PageUp => picker.selected = picker.selected.saturating_sub(10),
            KeyCode::PageDown => picker.selected = (picker.selected + 10).min(rows - 1),
            KeyCode::Left => {
                if let Some(parent) = picker.dir.parent().map(Path::to_path_buf) {
                    picker.go(parent);
                }
            }
            KeyCode::Backspace if picker.filter.is_empty() => {
                if let Some(parent) = picker.dir.parent().map(Path::to_path_buf) {
                    picker.go(parent);
                }
            }
            KeyCode::Backspace => {
                picker.filter.pop();
                picker.selected = usize::from(!picker.visible().is_empty());
            }
            KeyCode::Right => {
                if let Some(path) = picker.selected_entry().map(|e| e.path.clone()) {
                    picker.go(path);
                }
            }
            KeyCode::Enter if picker.is_path_filter() => {
                let path = config::expand_home(Path::new(picker.filter.trim()));
                if path.is_dir() {
                    picker.go(path);
                } else {
                    picker.error = Some(format!("{} is not a folder.", picker.filter.trim()));
                }
            }
            KeyCode::Enter => match picker.selected_entry().map(|e| e.path.clone()) {
                Some(path) => picker.go(path),
                None => return (None, Some(add(&picker.dir))),
            },
            KeyCode::Tab => {
                let path = picker
                    .selected_entry()
                    .map_or_else(|| picker.dir.clone(), |e| e.path.clone());
                return (None, Some(add(&path)));
            }
            KeyCode::Char('~') if picker.filter.is_empty() => {
                if let Some(home) = std::env::var_os("HOME") {
                    picker.go(PathBuf::from(home));
                }
            }
            KeyCode::Char(c) if !c.is_control() => {
                picker.filter.push(c);
                picker.error = None;
                picker.selected =
                    usize::from(!picker.visible().is_empty() && !picker.is_path_filter());
            }
            _ => {}
        }
        (Some(Overlay::Folders(picker)), None)
    }
}

fn add(path: &Path) -> Effect {
    Effect::Send(Request::AddProject(path.to_owned()))
}

pub fn render(app: &App, picker: &FolderPicker, area: Rect, buf: &mut Buffer) {
    let theme = app.theme;
    let width = 72u16.min(area.width.saturating_sub(2));
    let height = 24u16.min(area.height.saturating_sub(2));
    let inner = width.saturating_sub(4) as usize;
    let list_rows = (height as usize).saturating_sub(9).max(3);

    let added = |path: &Path| {
        let path = path.canonicalize().unwrap_or_else(|_| path.to_owned());
        app.snapshot.projects.iter().any(|p| p.path == path)
    };
    let badge = |kind: Option<&str>, path: &Path| -> Span<'static> {
        if added(path) {
            Span::styled("added", theme.success())
        } else {
            match kind {
                Some(kind) => Span::styled(kind.to_owned(), theme.accent()),
                None => Span::raw(""),
            }
        }
    };

    let mut lines = vec![
        Line::styled(format!(" {}", tilde(&picker.dir)), theme.accent()),
        if picker.filter.is_empty() {
            Line::styled(" type to filter, ~ for home", theme.dim())
        } else {
            Line::from(vec![
                Span::styled(" filter: ", theme.dim()),
                Span::styled(picker.filter.clone(), theme.text()),
                Span::styled("█", theme.accent()),
            ])
        },
        Line::raw(""),
    ];

    let visible = picker.visible();
    let mut rows: Vec<(String, Span<'static>)> = vec![(
        format!("+ Add this folder ({})", detect::project_name(&picker.dir)),
        badge(detect::project_kind(&picker.dir), &picker.dir),
    )];
    rows.extend(
        visible
            .iter()
            .map(|e| (format!("▸ {}/", e.name), badge(e.kind, &e.path))),
    );
    let first = picker.selected.saturating_sub(list_rows - 1);
    for (i, (label, badge)) in rows.iter().enumerate().skip(first).take(list_rows) {
        let style = if i == picker.selected {
            theme.selected()
        } else {
            theme.text()
        };
        let label_width = inner.saturating_sub(10);
        lines.push(Line::from(vec![
            Span::styled(format!(" {}", fit(label, label_width)), style),
            Span::raw(" "),
            badge.clone(),
        ]));
    }
    if visible.is_empty() && !picker.filter.is_empty() && !picker.is_path_filter() {
        lines.push(Line::styled("   no folder matches", theme.dim()));
    }
    while lines.len() < 3 + list_rows {
        lines.push(Line::raw(""));
    }
    lines.push(Line::raw(""));
    if let Some(error) = &picker.error {
        lines.push(Line::styled(format!(" {error}"), theme.error()));
    }
    lines.push(Line::from(vec![
        Span::styled(" enter", theme.key()),
        Span::styled(" open  ", theme.dim()),
        Span::styled("tab", theme.key()),
        Span::styled(" add selected  ", theme.dim()),
        Span::styled("←", theme.key()),
        Span::styled(" up  ", theme.dim()),
        Span::styled("esc", theme.key()),
        Span::styled(" cancel", theme.dim()),
    ]));

    let popup = centered(area, width, height);
    Clear.render(popup, buf);
    Paragraph::new(lines)
        .block(
            Block::bordered()
                .title(Span::styled(" Add a project ", theme.title()))
                .border_style(theme.focused_border())
                .style(theme.base()),
        )
        .render(popup, buf);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        for dir in ["shop", "notes", "api", ".cache"] {
            fs::create_dir(root.path().join(dir)).unwrap();
        }
        fs::write(root.path().join("shop/package.json"), "{}").unwrap();
        fs::write(root.path().join("api/Cargo.toml"), "").unwrap();
        root
    }

    fn names(picker: &FolderPicker) -> Vec<String> {
        picker.visible().iter().map(|e| e.name.clone()).collect()
    }

    #[test]
    fn projects_come_first_and_hidden_folders_stay_hidden() {
        let root = tree();
        let picker = FolderPicker::open(root.path().to_owned());
        assert_eq!(names(&picker), vec!["api", "shop", "notes"]);
    }

    #[test]
    fn typing_filters_and_selects_the_first_match() {
        let root = tree();
        let mut picker = FolderPicker::open(root.path().to_owned());
        picker.filter = "sh".into();
        assert_eq!(names(&picker), vec!["shop"]);
        picker.filter = ".c".into();
        assert_eq!(names(&picker), vec![".cache"]);
    }
}
