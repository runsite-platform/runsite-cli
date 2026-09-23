use super::status::{Glyph, Tone};
use chrono::{DateTime, Utc};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::border;

const ACCENT: (u8, u8, u8, u8) = (99, 102, 241, 63);
const GOOD: (u8, u8, u8, u8) = (34, 197, 94, 35);
const BAD: (u8, u8, u8, u8) = (239, 68, 68, 196);
const WARN: (u8, u8, u8, u8) = (234, 179, 8, 178);
const MUTED: (u8, u8, u8, u8) = (107, 114, 128, 243);

const UNICODE_SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const ASCII_SPINNER: [&str; 4] = ["|", "/", "-", "\\"];

const ASCII_BORDER: border::Set = border::Set {
    top_left: "+",
    top_right: "+",
    bottom_left: "+",
    bottom_right: "+",
    vertical_left: "|",
    vertical_right: "|",
    horizontal_top: "-",
    horizontal_bottom: "-",
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ColorMode {
    None,
    Indexed,
    TrueColor,
}

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    color: ColorMode,
    pub ascii: bool,
}

impl Theme {
    pub fn detect() -> Self {
        Self::from_env(|key| std::env::var(key).ok())
    }

    pub fn from_env(read: impl Fn(&str) -> Option<String>) -> Self {
        let dumb = read("TERM").as_deref() == Some("dumb");
        let ascii = dumb || read("RUNSITE_ASCII").as_deref() == Some("1");
        let no_color = read("NO_COLOR").is_some_and(|value| !value.is_empty());
        let color = if no_color || dumb {
            ColorMode::None
        } else if matches!(read("COLORTERM").as_deref(), Some("truecolor" | "24bit")) {
            ColorMode::TrueColor
        } else {
            ColorMode::Indexed
        };
        Self { color, ascii }
    }

    fn paint(&self, (red, green, blue, indexed): (u8, u8, u8, u8)) -> Style {
        match self.color {
            ColorMode::None => Style::default(),
            ColorMode::Indexed => Style::default().fg(Color::Indexed(indexed)),
            ColorMode::TrueColor => Style::default().fg(Color::Rgb(red, green, blue)),
        }
    }

    pub fn accent(&self) -> Style {
        self.paint(ACCENT).add_modifier(Modifier::BOLD)
    }

    pub fn muted(&self) -> Style {
        self.paint(MUTED)
    }

    pub fn warning(&self) -> Style {
        self.paint(WARN)
    }

    pub fn tone(&self, tone: Tone) -> Style {
        match tone {
            Tone::Good => self.paint(GOOD),
            Tone::Bad => self.paint(BAD),
            Tone::Muted => self.paint(MUTED),
            Tone::Neutral => Style::default(),
        }
    }

    pub fn selected(&self) -> Style {
        Style::default().add_modifier(Modifier::REVERSED)
    }

    pub fn border(&self, focused: bool) -> Style {
        if focused {
            self.paint(ACCENT)
        } else {
            self.muted()
        }
    }

    pub fn border_set(&self) -> border::Set<'static> {
        if self.ascii {
            ASCII_BORDER
        } else {
            border::PLAIN
        }
    }

    pub fn glyph(&self, glyph: Glyph, now: DateTime<Utc>) -> &'static str {
        if glyph == Glyph::Busy {
            return self.spinner(now);
        }
        match (glyph, self.ascii) {
            (Glyph::Running, false) => "●",
            (Glyph::Running, true) => "*",
            (Glyph::Stopped, false) => "○",
            (Glyph::Stopped, true) => "o",
            (Glyph::Sleeping, false) => "◐",
            (Glyph::Sleeping, true) => "z",
            (Glyph::Failed, false) => "✕",
            (Glyph::Failed, true) => "x",
            (Glyph::Blocked, false) => "⊘",
            (Glyph::Blocked, true) => "!",
            (Glyph::Unknown, _) | (Glyph::Busy, _) => "?",
        }
    }

    pub fn spinner(&self, now: DateTime<Utc>) -> &'static str {
        let step = (now.timestamp_millis() / 100).unsigned_abs() as usize;
        if self.ascii {
            ASCII_SPINNER[step % ASCII_SPINNER.len()]
        } else {
            UNICODE_SPINNER[step % UNICODE_SPINNER.len()]
        }
    }

    pub fn database_marker(&self) -> &'static str {
        if self.ascii {
            "#"
        } else {
            "◆"
        }
    }

    pub fn pointer(&self) -> &'static str {
        if self.ascii {
            ">"
        } else {
            "▸"
        }
    }

    pub fn separator(&self) -> &'static str {
        if self.ascii {
            " | "
        } else {
            " · "
        }
    }

    pub fn range_dash(&self) -> &'static str {
        if self.ascii {
            "-"
        } else {
            "–"
        }
    }

    pub fn ellipsis(&self) -> &'static str {
        if self.ascii {
            "..."
        } else {
            "…"
        }
    }

    pub fn cursor(&self) -> &'static str {
        if self.ascii {
            "_"
        } else {
            "▏"
        }
    }

    pub fn logo(&self) -> [&'static str; 2] {
        if self.ascii {
            ["[RS]", "    "]
        } else {
            ["█▀▄ █▀▀", "█▀▄ ▄▄█"]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn theme_with(variables: &[(&str, &str)]) -> Theme {
        let variables: HashMap<String, String> = variables
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();
        Theme::from_env(|key| variables.get(key).cloned())
    }

    #[test]
    fn truecolor_is_used_only_when_advertised() {
        assert_eq!(
            theme_with(&[("COLORTERM", "truecolor")]).color,
            ColorMode::TrueColor
        );
        assert_eq!(
            theme_with(&[("COLORTERM", "24bit")]).color,
            ColorMode::TrueColor
        );
        assert_eq!(theme_with(&[]).color, ColorMode::Indexed);
    }

    #[test]
    fn no_color_disables_colors_but_keeps_unicode() {
        let theme = theme_with(&[("NO_COLOR", "1"), ("COLORTERM", "truecolor")]);
        assert_eq!(theme.color, ColorMode::None);
        assert!(!theme.ascii);
        assert_eq!(theme.tone(Tone::Good), Style::default());
    }

    #[test]
    fn a_dumb_terminal_gets_ascii_without_colors() {
        let theme = theme_with(&[("TERM", "dumb")]);
        assert!(theme.ascii);
        assert_eq!(theme.color, ColorMode::None);
        assert_eq!(theme.glyph(Glyph::Running, Utc::now()), "*");
    }

    #[test]
    fn runsite_ascii_switches_glyphs_only() {
        let theme = theme_with(&[("RUNSITE_ASCII", "1")]);
        assert!(theme.ascii);
        assert_eq!(theme.color, ColorMode::Indexed);
        assert_eq!(theme.pointer(), ">");
        assert_eq!(theme.border_set().top_left, "+");
    }
}
