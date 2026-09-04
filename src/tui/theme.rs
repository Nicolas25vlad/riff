use std::env;

use ratatui::style::Color;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub name: &'static str,
    pub background: Color,
    pub foreground: Color,
    pub accent: Color,
    pub muted: Color,
    pub border: Color,
    pub selection: Color,
    pub danger: Color,
}

const THEMES: [Theme; 4] = [
    Theme {
        name: "riff",
        background: Color::Reset,
        foreground: Color::Reset,
        accent: Color::Cyan,
        muted: Color::DarkGray,
        border: Color::DarkGray,
        selection: Color::Blue,
        danger: Color::Red,
    },
    Theme {
        name: "kanagawa",
        background: Color::Rgb(31, 31, 40),
        foreground: Color::Rgb(220, 215, 186),
        accent: Color::Rgb(230, 195, 132),
        muted: Color::Rgb(114, 113, 105),
        border: Color::Rgb(84, 84, 109),
        selection: Color::Rgb(47, 79, 79),
        danger: Color::Rgb(195, 64, 67),
    },
    Theme {
        name: "catppuccin",
        background: Color::Rgb(30, 30, 46),
        foreground: Color::Rgb(205, 214, 244),
        accent: Color::Rgb(203, 166, 247),
        muted: Color::Rgb(127, 132, 156),
        border: Color::Rgb(88, 91, 112),
        selection: Color::Rgb(49, 50, 68),
        danger: Color::Rgb(243, 139, 168),
    },
    Theme {
        name: "mono",
        background: Color::Reset,
        foreground: Color::White,
        accent: Color::White,
        muted: Color::DarkGray,
        border: Color::Gray,
        selection: Color::DarkGray,
        danger: Color::White,
    },
];

impl Theme {
    pub fn from_env() -> Self {
        let requested = env::var("RIFF_THEME").unwrap_or_else(|_| "riff".to_string());
        Self::named(&requested).unwrap_or(THEMES[0])
    }

    pub fn named(name: &str) -> Option<Self> {
        THEMES
            .iter()
            .copied()
            .find(|theme| theme.name.eq_ignore_ascii_case(name))
    }

    pub fn next(self) -> Self {
        let index = THEMES
            .iter()
            .position(|theme| theme.name == self.name)
            .unwrap_or(0);
        THEMES[(index + 1) % THEMES.len()]
    }

    pub fn names() -> impl Iterator<Item = &'static str> {
        THEMES.iter().map(|theme| theme.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_named_theme_case_insensitively() {
        assert_eq!(Theme::named("KANAGAWA").unwrap().name, "kanagawa");
    }

    #[test]
    fn theme_cycle_wraps() {
        let mut theme = THEMES[0];
        for _ in 0..THEMES.len() {
            theme = theme.next();
        }
        assert_eq!(theme, THEMES[0]);
    }
}
