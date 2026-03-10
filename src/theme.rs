use std::fmt;
use std::str::FromStr;

use anyhow::{Result, bail};
use crossterm::style::Color as CtColor;
use ratatui::style::Color as RatColor;

/// A color theme for devpulse output.
///
/// Each field represents a semantic color role used across
/// the table and TUI renderers.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    /// Theme name (for display purposes).
    pub name: String,
    /// Header/title text.
    pub header: ThemeColor,
    /// Clean/good status.
    pub clean: ThemeColor,
    /// Dirty/warning status.
    pub dirty: ThemeColor,
    /// Stale/error items (e.g. old commits).
    pub stale: ThemeColor,
    /// Dimmed/secondary text.
    pub dim: ThemeColor,
    /// Accent color (highlights, selected items, CI pass).
    pub accent: ThemeColor,
    /// Background for highlighted rows in TUI.
    pub highlight_bg: ThemeColor,
}

/// A color that can be converted to both crossterm and ratatui color types.
#[derive(Debug, Clone, PartialEq)]
pub struct ThemeColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl ThemeColor {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Convert to a crossterm color (used in table output).
    pub fn to_crossterm(&self) -> CtColor {
        CtColor::Rgb {
            r: self.r,
            g: self.g,
            b: self.b,
        }
    }

    /// Convert to a ratatui color (used in TUI output).
    pub fn to_ratatui(&self) -> RatColor {
        RatColor::Rgb(self.r, self.g, self.b)
    }
}

/// Built-in theme names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeName {
    Default,
    Dracula,
    CatppuccinMocha,
    Nord,
}

impl fmt::Display for ThemeName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Default => write!(f, "default"),
            Self::Dracula => write!(f, "dracula"),
            Self::CatppuccinMocha => write!(f, "catppuccin-mocha"),
            Self::Nord => write!(f, "nord"),
        }
    }
}

impl FromStr for ThemeName {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.trim().to_lowercase().as_str() {
            "default" => Ok(Self::Default),
            "dracula" => Ok(Self::Dracula),
            "catppuccin-mocha" | "catppuccin" | "mocha" => Ok(Self::CatppuccinMocha),
            "nord" => Ok(Self::Nord),
            other => bail!(
                "Unknown theme: '{}'. Available themes: default, dracula, catppuccin-mocha, nord",
                other
            ),
        }
    }
}

/// Return the built-in theme for the given name.
pub fn builtin_theme(name: ThemeName) -> Theme {
    match name {
        ThemeName::Default => default_theme(),
        ThemeName::Dracula => dracula_theme(),
        ThemeName::CatppuccinMocha => catppuccin_mocha_theme(),
        ThemeName::Nord => nord_theme(),
    }
}

/// Resolve a theme from an optional theme name string.
///
/// Returns the built-in theme matching the name, or the default theme
/// if no name is provided.
pub fn resolve_theme(name: Option<&str>) -> Result<Theme> {
    match name {
        Some(s) => {
            let theme_name = s.parse::<ThemeName>()?;
            Ok(builtin_theme(theme_name))
        }
        None => Ok(default_theme()),
    }
}

// ── Built-in Themes ─────────────────────────────────────────────────

fn default_theme() -> Theme {
    Theme {
        name: "default".to_string(),
        header: ThemeColor::new(255, 255, 255),    // white
        clean: ThemeColor::new(0, 255, 0),         // green
        dirty: ThemeColor::new(255, 255, 0),       // yellow
        stale: ThemeColor::new(255, 0, 0),         // red
        dim: ThemeColor::new(128, 128, 128),       // grey
        accent: ThemeColor::new(0, 255, 255),      // cyan
        highlight_bg: ThemeColor::new(68, 68, 68), // dark grey
    }
}

/// Dracula color palette: https://draculatheme.com/
fn dracula_theme() -> Theme {
    Theme {
        name: "dracula".to_string(),
        header: ThemeColor::new(189, 147, 249),    // purple
        clean: ThemeColor::new(80, 250, 123),      // green
        dirty: ThemeColor::new(255, 184, 108),     // orange
        stale: ThemeColor::new(255, 85, 85),       // red
        dim: ThemeColor::new(98, 114, 164),        // comment grey
        accent: ThemeColor::new(139, 233, 253),    // cyan
        highlight_bg: ThemeColor::new(68, 71, 90), // current line
    }
}

/// Catppuccin Mocha palette: https://catppuccin.com/
fn catppuccin_mocha_theme() -> Theme {
    Theme {
        name: "catppuccin-mocha".to_string(),
        header: ThemeColor::new(203, 166, 247),    // mauve
        clean: ThemeColor::new(166, 227, 161),     // green
        dirty: ThemeColor::new(249, 226, 175),     // yellow
        stale: ThemeColor::new(243, 139, 168),     // red
        dim: ThemeColor::new(127, 132, 156),       // overlay0
        accent: ThemeColor::new(137, 220, 235),    // teal
        highlight_bg: ThemeColor::new(49, 50, 68), // surface0
    }
}

/// Nord palette: https://www.nordtheme.com/
fn nord_theme() -> Theme {
    Theme {
        name: "nord".to_string(),
        header: ThemeColor::new(136, 192, 208), // nord8 (frost)
        clean: ThemeColor::new(163, 190, 140),  // nord14 (aurora green)
        dirty: ThemeColor::new(235, 203, 139),  // nord13 (aurora yellow)
        stale: ThemeColor::new(191, 97, 106),   // nord11 (aurora red)
        dim: ThemeColor::new(76, 86, 106),      // nord3 (polar night)
        accent: ThemeColor::new(129, 161, 193), // nord9 (frost)
        highlight_bg: ThemeColor::new(59, 66, 82), // nord1
    }
}

/// List all available theme names.
pub fn available_themes() -> Vec<ThemeName> {
    vec![
        ThemeName::Default,
        ThemeName::Dracula,
        ThemeName::CatppuccinMocha,
        ThemeName::Nord,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_name_from_str_valid() {
        assert_eq!("default".parse::<ThemeName>().unwrap(), ThemeName::Default);
        assert_eq!("dracula".parse::<ThemeName>().unwrap(), ThemeName::Dracula);
        assert_eq!(
            "catppuccin-mocha".parse::<ThemeName>().unwrap(),
            ThemeName::CatppuccinMocha
        );
        assert_eq!(
            "catppuccin".parse::<ThemeName>().unwrap(),
            ThemeName::CatppuccinMocha
        );
        assert_eq!(
            "mocha".parse::<ThemeName>().unwrap(),
            ThemeName::CatppuccinMocha
        );
        assert_eq!("nord".parse::<ThemeName>().unwrap(), ThemeName::Nord);
    }

    #[test]
    fn test_theme_name_from_str_case_insensitive() {
        assert_eq!("DRACULA".parse::<ThemeName>().unwrap(), ThemeName::Dracula);
        assert_eq!("Nord".parse::<ThemeName>().unwrap(), ThemeName::Nord);
        assert_eq!(
            "Catppuccin-Mocha".parse::<ThemeName>().unwrap(),
            ThemeName::CatppuccinMocha
        );
    }

    #[test]
    fn test_theme_name_from_str_with_whitespace() {
        assert_eq!(
            "  dracula  ".parse::<ThemeName>().unwrap(),
            ThemeName::Dracula
        );
    }

    #[test]
    fn test_theme_name_from_str_invalid() {
        let result = "solarized".parse::<ThemeName>();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("solarized"));
        assert!(err.contains("Available themes"));
    }

    #[test]
    fn test_theme_name_display() {
        assert_eq!(ThemeName::Default.to_string(), "default");
        assert_eq!(ThemeName::Dracula.to_string(), "dracula");
        assert_eq!(ThemeName::CatppuccinMocha.to_string(), "catppuccin-mocha");
        assert_eq!(ThemeName::Nord.to_string(), "nord");
    }

    #[test]
    fn test_builtin_theme_returns_correct_name() {
        assert_eq!(builtin_theme(ThemeName::Default).name, "default");
        assert_eq!(builtin_theme(ThemeName::Dracula).name, "dracula");
        assert_eq!(
            builtin_theme(ThemeName::CatppuccinMocha).name,
            "catppuccin-mocha"
        );
        assert_eq!(builtin_theme(ThemeName::Nord).name, "nord");
    }

    #[test]
    fn test_resolve_theme_none_returns_default() {
        let theme = resolve_theme(None).unwrap();
        assert_eq!(theme.name, "default");
    }

    #[test]
    fn test_resolve_theme_valid_name() {
        let theme = resolve_theme(Some("dracula")).unwrap();
        assert_eq!(theme.name, "dracula");
    }

    #[test]
    fn test_resolve_theme_invalid_name() {
        let result = resolve_theme(Some("nonexistent"));
        assert!(result.is_err());
    }

    #[test]
    fn test_theme_color_to_crossterm() {
        let color = ThemeColor::new(255, 128, 0);
        let ct = color.to_crossterm();
        assert_eq!(
            ct,
            CtColor::Rgb {
                r: 255,
                g: 128,
                b: 0
            }
        );
    }

    #[test]
    fn test_theme_color_to_ratatui() {
        let color = ThemeColor::new(100, 200, 50);
        let rat = color.to_ratatui();
        assert_eq!(rat, RatColor::Rgb(100, 200, 50));
    }

    #[test]
    fn test_all_themes_have_distinct_colors() {
        let themes: Vec<Theme> = available_themes()
            .iter()
            .map(|n| builtin_theme(*n))
            .collect();
        // Each theme should have different header color
        for i in 0..themes.len() {
            for j in (i + 1)..themes.len() {
                assert_ne!(
                    themes[i].header, themes[j].header,
                    "Themes {} and {} should have different header colors",
                    themes[i].name, themes[j].name
                );
            }
        }
    }

    #[test]
    fn test_available_themes_length() {
        assert_eq!(available_themes().len(), 4);
    }

    #[test]
    fn test_dracula_colors_match_spec() {
        let theme = dracula_theme();
        // Dracula green
        assert_eq!(theme.clean, ThemeColor::new(80, 250, 123));
        // Dracula purple
        assert_eq!(theme.header, ThemeColor::new(189, 147, 249));
        // Dracula red
        assert_eq!(theme.stale, ThemeColor::new(255, 85, 85));
    }

    #[test]
    fn test_catppuccin_mocha_colors_match_spec() {
        let theme = catppuccin_mocha_theme();
        // Catppuccin green
        assert_eq!(theme.clean, ThemeColor::new(166, 227, 161));
        // Catppuccin mauve
        assert_eq!(theme.header, ThemeColor::new(203, 166, 247));
    }

    #[test]
    fn test_nord_colors_match_spec() {
        let theme = nord_theme();
        // Nord aurora green
        assert_eq!(theme.clean, ThemeColor::new(163, 190, 140));
        // Nord frost
        assert_eq!(theme.header, ThemeColor::new(136, 192, 208));
    }
}
