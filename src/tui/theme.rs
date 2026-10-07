//! Every colour and style the TUI uses. View code asks the theme, never
//! builds a `Color` itself.
//!
//! Themes are palettes. `terminal` uses the terminal's own 16 colours and
//! background; the rest paint their own background. Terminals that do not
//! announce 24-bit colour (`COLORTERM=truecolor`) get the nearest of the 256
//! standard colours instead, which keeps the themes recognisable in macOS
//! Terminal. With `NO_COLOR` set, only bold, dim and reverse are used.
//!
//! The mascot keeps its own colours in every theme: it is the brand.

use ratatui::style::{Color, Modifier, Style};

use crate::model::ProcessState;

#[derive(Debug)]
pub struct Palette {
    pub name: &'static str,
    /// `None` keeps the terminal's background.
    bg: Option<Color>,
    fg: Color,
    dim: Color,
    border: Color,
    accent: Color,
    selection: Color,
    green: Color,
    yellow: Color,
    red: Color,
    info: Color,
}

const fn rgb(hex: u32) -> Color {
    Color::Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

pub const PALETTES: &[Palette] = &[
    Palette {
        name: "paddock",
        bg: Some(rgb(0x15171c)),
        fg: rgb(0xe6e1d6),
        dim: rgb(0x7d8590),
        border: rgb(0x343a46),
        accent: rgb(0xf0883e),
        selection: rgb(0x2b3140),
        green: rgb(0x7ee787),
        yellow: rgb(0xe3b341),
        red: rgb(0xff7b72),
        info: rgb(0x79c0ff),
    },
    Palette {
        name: "terminal",
        bg: None,
        fg: Color::Reset,
        dim: Color::DarkGray,
        border: Color::DarkGray,
        accent: Color::Cyan,
        selection: Color::DarkGray,
        green: Color::Green,
        yellow: Color::Yellow,
        red: Color::Red,
        info: Color::Blue,
    },
    Palette {
        name: "catppuccin-mocha",
        bg: Some(rgb(0x1e1e2e)),
        fg: rgb(0xcdd6f4),
        dim: rgb(0x6c7086),
        border: rgb(0x45475a),
        accent: rgb(0xcba6f7),
        selection: rgb(0x313244),
        green: rgb(0xa6e3a1),
        yellow: rgb(0xf9e2af),
        red: rgb(0xf38ba8),
        info: rgb(0x89b4fa),
    },
    Palette {
        name: "dracula",
        bg: Some(rgb(0x282a36)),
        fg: rgb(0xf8f8f2),
        dim: rgb(0x6272a4),
        border: rgb(0x44475a),
        accent: rgb(0xbd93f9),
        selection: rgb(0x44475a),
        green: rgb(0x50fa7b),
        yellow: rgb(0xf1fa8c),
        red: rgb(0xff5555),
        info: rgb(0x8be9fd),
    },
    Palette {
        name: "nord",
        bg: Some(rgb(0x2e3440)),
        fg: rgb(0xd8dee9),
        dim: rgb(0x616e88),
        border: rgb(0x4c566a),
        accent: rgb(0x88c0d0),
        selection: rgb(0x3b4252),
        green: rgb(0xa3be8c),
        yellow: rgb(0xebcb8b),
        red: rgb(0xbf616a),
        info: rgb(0x81a1c1),
    },
    Palette {
        name: "gruvbox",
        bg: Some(rgb(0x282828)),
        fg: rgb(0xebdbb2),
        dim: rgb(0x928374),
        border: rgb(0x504945),
        accent: rgb(0xfe8019),
        selection: rgb(0x3c3836),
        green: rgb(0xb8bb26),
        yellow: rgb(0xfabd2f),
        red: rgb(0xfb4934),
        info: rgb(0x83a598),
    },
    Palette {
        name: "tokyo-night",
        bg: Some(rgb(0x1a1b26)),
        fg: rgb(0xc0caf5),
        dim: rgb(0x565f89),
        border: rgb(0x3b4261),
        accent: rgb(0x7aa2f7),
        selection: rgb(0x283457),
        green: rgb(0x9ece6a),
        yellow: rgb(0xe0af68),
        red: rgb(0xf7768e),
        info: rgb(0x7dcfff),
    },
    Palette {
        name: "solarized-light",
        bg: Some(rgb(0xfdf6e3)),
        fg: rgb(0x586e75),
        dim: rgb(0x93a1a1),
        border: rgb(0xd3cbb7),
        accent: rgb(0x268bd2),
        selection: rgb(0xeee8d5),
        green: rgb(0x859900),
        yellow: rgb(0xb58900),
        red: rgb(0xdc322f),
        info: rgb(0x2aa198),
    },
];

/// Mascot colours, the same in every theme.
pub mod mascot {
    use super::rgb;
    use ratatui::style::Color;

    pub const COAT: Color = rgb(0xe8893a);
    pub const MANE: Color = rgb(0x6b3410);
    pub const BLAZE: Color = rgb(0xfff4e2);
    pub const MUZZLE: Color = rgb(0xf3c08f);
    pub const DARK: Color = rgb(0x1b1410);
}

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    palette: &'static Palette,
    color: bool,
    truecolor: bool,
}

impl Theme {
    /// The named theme, or the default one if the name is unknown.
    pub fn named(name: &str) -> Self {
        let palette = PALETTES
            .iter()
            .find(|p| p.name == name)
            .unwrap_or(&PALETTES[0]);
        Self {
            palette,
            color: std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty()),
            truecolor: std::env::var("COLORTERM").is_ok_and(|v| v == "truecolor" || v == "24bit"),
        }
    }

    pub fn exists(name: &str) -> bool {
        PALETTES.iter().any(|p| p.name == name)
    }

    #[cfg(test)]
    pub fn plain() -> Self {
        Self {
            palette: &PALETTES[0],
            color: false,
            truecolor: false,
        }
    }

    pub fn name(self) -> &'static str {
        self.palette.name
    }

    pub fn has_color(self) -> bool {
        self.color
    }

    /// Converts for the terminal: 24-bit if supported, else the 256 palette.
    pub fn color(self, color: Color) -> Color {
        match color {
            Color::Rgb(r, g, b) if !self.truecolor => Color::Indexed(nearest_256(r, g, b)),
            other => other,
        }
    }

    fn fg(self, color: Color) -> Style {
        if self.color {
            Style::new().fg(self.color(color))
        } else {
            Style::new()
        }
    }

    /// Background and text colour for every cell the TUI owns.
    pub fn base(self) -> Style {
        if !self.color {
            return Style::new();
        }
        let style = Style::new().fg(self.color(self.palette.fg));
        match self.palette.bg {
            Some(bg) => style.bg(self.color(bg)),
            None => style,
        }
    }

    pub fn text(self) -> Style {
        self.fg(self.palette.fg)
    }

    pub fn accent(self) -> Style {
        self.fg(self.palette.accent).add_modifier(Modifier::BOLD)
    }

    pub fn border(self) -> Style {
        self.fg(self.palette.border)
    }

    pub fn focused_border(self) -> Style {
        self.fg(self.palette.accent)
    }

    pub fn selected(self) -> Style {
        if self.color && self.palette.bg.is_some() {
            Style::new()
                .bg(self.color(self.palette.selection))
                .add_modifier(Modifier::BOLD)
        } else {
            Style::new().add_modifier(Modifier::REVERSED)
        }
    }

    /// Lines picked for copying in the log pane.
    pub fn marked(self) -> Style {
        if self.color {
            Style::new()
                .bg(self.color(self.palette.selection))
                .fg(self.color(self.palette.accent))
        } else {
            Style::new().add_modifier(Modifier::REVERSED)
        }
    }

    pub fn dim(self) -> Style {
        if self.color {
            self.fg(self.palette.dim)
        } else {
            Style::new().add_modifier(Modifier::DIM)
        }
    }

    pub fn title(self) -> Style {
        self.text().add_modifier(Modifier::BOLD)
    }

    pub fn key(self) -> Style {
        self.fg(self.palette.accent).add_modifier(Modifier::BOLD)
    }

    pub fn info(self) -> Style {
        self.fg(self.palette.info)
    }

    pub fn success(self) -> Style {
        self.fg(self.palette.green)
    }

    pub fn error(self) -> Style {
        self.fg(self.palette.red)
    }

    pub fn warning(self) -> Style {
        self.fg(self.palette.yellow)
    }

    pub fn state(self, state: ProcessState) -> Style {
        match state {
            ProcessState::Running => self.fg(self.palette.green),
            ProcessState::Starting | ProcessState::Stopping => self.fg(self.palette.yellow),
            ProcessState::Crashed(_) => self.fg(self.palette.red).add_modifier(Modifier::BOLD),
            ProcessState::Stopped | ProcessState::Exited => self.dim(),
        }
    }

    pub fn glyph(state: ProcessState) -> &'static str {
        match state {
            ProcessState::Running => "●",
            ProcessState::Starting | ProcessState::Stopping => "◐",
            ProcessState::Crashed(_) => "✗",
            ProcessState::Stopped | ProcessState::Exited => "○",
        }
    }
}

/// Nearest colour in the xterm 256 palette (6x6x6 cube plus grey ramp).
fn nearest_256(r: u8, g: u8, b: u8) -> u8 {
    const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    let level = |v: u8| {
        (0..6)
            .min_by_key(|&i| (LEVELS[i] as i32 - v as i32).abs())
            .unwrap_or(0)
    };
    let (ri, gi, bi) = (level(r), level(g), level(b));
    let cube = 16 + 36 * ri + 6 * gi + bi;
    let cube_rgb = (LEVELS[ri], LEVELS[gi], LEVELS[bi]);

    let average = (r as u32 + g as u32 + b as u32) / 3;
    let grey_index = ((average.saturating_sub(8)) / 10).min(23) as u8;
    let grey = 8 + 10 * grey_index;

    let distance = |(cr, cg, cb): (u8, u8, u8)| {
        let d = |a: u8, b: u8| (a as i32 - b as i32).pow(2);
        d(cr, r) + d(cg, g) + d(cb, b)
    };
    if distance((grey, grey, grey)) < distance(cube_rgb) {
        232 + grey_index
    } else {
        cube as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_names_are_unique() {
        let mut names: Vec<_> = PALETTES.iter().map(|p| p.name).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), PALETTES.len());
    }

    #[test]
    fn unknown_theme_falls_back_to_the_default() {
        assert_eq!(Theme::named("no-such-theme").name(), "paddock");
    }

    #[test]
    fn nearest_256_picks_exact_cube_and_grey_entries() {
        assert_eq!(nearest_256(255, 0, 0), 196);
        assert_eq!(nearest_256(0, 0, 0), 16);
        assert_eq!(nearest_256(128, 128, 128), 244);
    }
}
