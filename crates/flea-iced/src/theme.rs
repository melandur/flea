// The Omarchy palette, read from the same colors.toml ui/Theme.qml reads, with ui/js/Palette.js
// roles: muted, symlink, executable and error each lifted to a WCAG ratio against the background.
// An absent or unreadable file falls back to the shipped dark set.
use crate::contrast::{darker, ensure_ratio};
use iced::Color;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub background: Color,
    pub surface: Color,
    pub foreground: Color,
    pub muted: Color,
    // Row.qml's dim: the row mark's ink, Qt.darker(foreground, 1.4) and not lifted.
    pub dim: Color,
    pub accent: Color,
    pub selection: Color,
    pub symlink: Color,
    pub executable: Color,
    pub error: Color,
}

fn hex(s: &str) -> Option<Color> {
    let s = s.trim().trim_matches('"').strip_prefix('#')?;
    if s.len() != 6 {
        return None;
    }
    let n = u32::from_str_radix(s, 16).ok()?;
    Some(Color::from_rgb8((n >> 16) as u8, (n >> 8) as u8, n as u8))
}

fn lookup(body: &str, key: &str) -> Option<Color> {
    body.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        (k.trim() == key).then(|| hex(v)).flatten()
    })
}

impl Palette {
    pub fn from_toml(body: &str) -> Self {
        let d = Self::fallback();
        let get = |k: &str, f: Color| lookup(body, k).unwrap_or(f);
        // The surface ladder ThemeRoles.html specifies: dark_background, background, selection.
        let background = get("background", d.background);
        let surface = lookup(body, "dark_background")
            .or_else(|| lookup(body, "background"))
            .or_else(|| lookup(body, "selection"))
            .unwrap_or(d.surface);
        let foreground = get("foreground", d.foreground);
        // Palette.js: muted is the file's own key, else Qt.darker(foreground, 1.4), lifted to 3:1.
        let muted = lookup(body, "muted").unwrap_or_else(|| darker(foreground, 1.4));
        Palette {
            background,
            surface,
            foreground,
            muted: ensure_ratio(muted, background, 3.0),
            dim: darker(foreground, 1.4),
            accent: get("accent", d.accent),
            selection: get("selection", d.selection),
            symlink: ensure_ratio(get("cyan", get("color6", d.symlink)), background, 4.5),
            executable: ensure_ratio(get("green", get("color2", d.executable)), background, 4.5),
            error: ensure_ratio(get("red", get("color1", d.error)), background, 4.5),
        }
    }

    pub fn fallback() -> Self {
        let c = |s| hex(s).unwrap();
        Palette {
            background: c("#1a1b26"),
            surface: c("#13141c"),
            foreground: c("#a9b1d6"),
            muted: c("#565f89"),
            dim: darker(c("#a9b1d6"), 1.4),
            accent: c("#7aa2f7"),
            selection: c("#292e42"),
            symlink: c("#449dab"),
            executable: c("#9ece6a"),
            error: c("#f7768e"),
        }
    }

    // display.theme: "omarchy", "", or an id this catalog lost all follow the desktop. A catalog
    // palette's eight keys go first, so they win the lookup, and every key it does not state, the
    // selection fill among them, stays the desktop's (AGENTS.md "The theme catalog").
    pub fn load(theme_id: &str) -> Self {
        let path = std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|h| h.join(".local/state/omarchy/current/theme/colors.toml"));
        let desktop = path.and_then(|p| std::fs::read_to_string(p).ok());
        let catalog = palette_body(theme_id);
        match (catalog, desktop) {
            (None, None) => Self::fallback(),
            (Some(c), d) => Self::from_toml(&format!("{c}{}", d.unwrap_or_default())),
            (None, Some(d)) => Self::from_toml(&d),
        }
    }
}

use crate::tables as catalog;

// Themes.entries: every bundled palette as (id, name), in the catalog's own order.
pub fn entries() -> impl Iterator<Item = (&'static str, &'static str)> {
    catalog::THEMES.iter().map(|(id, name, _)| (*id, *name))
}

// Themes.resolve: an id the catalog carries, or None, which means following Omarchy.
pub fn resolve(id: &str) -> Option<&'static str> {
    catalog::THEMES.iter().find(|(i, _, _)| *i == id).map(|(i, _, _)| *i)
}

pub fn name_of(id: &str) -> Option<&'static str> {
    catalog::THEMES.iter().find(|(i, _, _)| *i == id).map(|(_, n, _)| *n)
}

// Themes.paletteBody: one palette as the colors.toml text from_toml already reads.
pub fn palette_body(id: &str) -> Option<String> {
    let (_, _, colours) = catalog::THEMES.iter().find(|(i, _, _)| *i == id)?;
    Some(catalog::THEME_KEYS.iter().zip(colours).map(|(k, v)| format!("{k} = \"{v}\"\n")).collect())
}

impl Palette {
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_catalog_palette_wins_its_eight_keys_and_leaves_the_rest_to_the_desktop() {
        assert_eq!(entries().count(), 95);
        assert_eq!(resolve("omarchy"), None);
        assert_eq!(name_of("catppuccin-mocha"), Some("Catppuccin Mocha"));
        let body = palette_body("3024").unwrap();
        assert!(body.starts_with("background = \"#090300\"\n"));
        let merged = Palette::from_toml(&format!("{body}background = \"#ffffff\"\nselection = \"#123456\"\n"));
        assert_eq!(merged.background, Color::from_rgb8(0x09, 0x03, 0x00));
        assert_eq!(merged.selection, Color::from_rgb8(0x12, 0x34, 0x56));
    }

    #[test]
    fn roles_come_from_the_file_and_the_rest_fall_back() {
        let p = Palette::from_toml("accent = \"#102030\"\nbackground = \"#000000\"\n");
        assert_eq!(p.accent, Color::from_rgb8(0x10, 0x20, 0x30));
        // No dark_background, so the surface ladder takes background next.
        assert_eq!(p.surface, Color::from_rgb8(0, 0, 0));
        assert_eq!(p.error, Palette::fallback().error);
    }
}
