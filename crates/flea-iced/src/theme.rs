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
    // A folder's name and a plain file's name; the foreground until Settings > Theme > Colours says.
    pub folder: Color,
    pub file: Color,
    // A folder's mark when the operator chose one, which wins over the filetype tier's colour.
    pub folder_mark: Option<Color>,
    // A file family's colour, for its name and its mark, in TYPES order.
    pub types: [Option<Color>; TYPES.len()],
}

// display.colors: each role the operator chose, None where the palette's own stands.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Overrides {
    pub cursor: Option<Color>,
    pub marked: Option<Color>,
    pub folder: Option<Color>,
    pub file: Option<Color>,
    pub executable: Option<Color>,
    pub symlink: Option<Color>,
    pub mark: Option<Color>,
    // display.typeColors, in TYPES order.
    pub types: [Option<Color>; TYPES.len()],
}

// display.typeColors: each file family by its mark, with the name Settings draws for it. The keys are
// src/uicolours.rs TYPE_COLORS in the same order.
pub const TYPES: [(&str, &str); 26] = [
    ("image", "Images"), ("film", "Video"), ("music", "Audio"), ("archive", "Archives"),
    ("file-text", "Documents and text"), ("table", "Spreadsheets"), ("presentation", "Presentations"),
    ("code", "Source code"), ("code-xml", "Markup"), ("braces", "Config and data"), ("terminal", "Scripts"),
    ("binary", "Binaries"), ("database", "Databases"), ("disc", "Disc images"), ("type", "Fonts"),
    ("key", "Keys and signatures"), ("hash", "Checksums"), ("book-open", "E-books"), ("palette", "Design files"),
    ("box", "3D models and packages"), ("captions", "Subtitles"), ("mail", "Mail"), ("calendar", "Calendars"),
    ("notebook", "Notebooks"), ("lock", "Lock files"), ("git-branch", "Git"),
];

impl Overrides {
    pub fn from_json(colors: Option<&serde_json::Value>) -> Self {
        let get = |k: &str| colors.and_then(|c| c.get(k)).and_then(serde_json::Value::as_str).and_then(hex);
        Overrides {
            cursor: get("cursor"),
            marked: get("marked"),
            folder: get("folder"),
            file: get("file"),
            executable: get("executable"),
            symlink: get("symlink"),
            mark: get("mark"),
            types: Self::types(None),
        }
    }

    pub fn from_doc(v: &serde_json::Value) -> Self {
        Overrides { types: Self::types(v.pointer("/display/typeColors")), ..Self::from_json(v.pointer("/display/colors")) }
    }

    fn types(map: Option<&serde_json::Value>) -> [Option<Color>; TYPES.len()] {
        TYPES.map(|(k, _)| map.and_then(|m| m.get(k)).and_then(serde_json::Value::as_str).and_then(hex))
    }
}

pub fn parse_hex(s: &str) -> Option<Color> {
    hex(s)
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
            folder: foreground,
            file: foreground,
            folder_mark: None,
            types: [None; TYPES.len()],
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
            folder: c("#a9b1d6"),
            file: c("#a9b1d6"),
            folder_mark: None,
            types: [None; TYPES.len()],
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
    // The palette with the operator's own colours laid over it, taken as chosen and never lifted:
    // the operator picked the colour against the background they are looking at.
    pub fn with(mut self, o: &Overrides) -> Self {
        self.accent = o.cursor.unwrap_or(self.accent);
        self.selection = o.marked.unwrap_or(self.selection);
        self.folder = o.folder.unwrap_or(self.folder);
        self.folder_mark = o.folder;
        self.file = o.file.unwrap_or(self.file);
        self.executable = o.executable.unwrap_or(self.executable);
        self.symlink = o.symlink.unwrap_or(self.symlink);
        self.dim = o.mark.unwrap_or(self.dim);
        self.types = o.types;
        self
    }

    // The colour the operator chose for this row's file family, if any.
    pub fn type_ink(&self, r: &crate::wire::Row) -> Option<Color> {
        let family = crate::marks::family(&r.i, r.p, &r.n, r.d)?;
        TYPES.iter().position(|(k, _)| *k == family).and_then(|i| self.types[i])
    }

    // A row's name: a link, its family's chosen colour, a folder, a program or a plain file.
    pub fn row_ink(&self, r: &crate::wire::Row) -> Color {
        if crate::format::is_symlink(r.p) { self.symlink } else { self.type_ink(r).unwrap_or_else(|| self.name_ink(r.d, r.p)) }
    }

    // The operator's own colour for a row's mark: a folder's, else its family's.
    pub fn row_mark(&self, r: &crate::wire::Row) -> Option<Color> {
        self.chosen_mark(r.d, r.p).or_else(|| self.type_ink(r))
    }

    // A row's name by kind alone: a link, a folder, a program or a plain file, each in its own role.
    pub fn name_ink(&self, is_dir: bool, mode: u32) -> Color {
        if crate::format::is_symlink(mode) {
            self.symlink
        } else if is_dir {
            self.folder
        } else if crate::format::is_executable(mode) {
            self.executable
        } else {
            self.file
        }
    }

    // The folder colour the operator chose, for a folder's own mark and never a link's.
    pub fn chosen_mark(&self, is_dir: bool, mode: u32) -> Option<Color> {
        self.folder_mark.filter(|_| is_dir && !crate::format::is_symlink(mode))
    }
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

    #[test]
    fn a_chosen_colour_replaces_its_role_and_an_empty_one_keeps_the_palette() {
        let o = Overrides::from_json(Some(&serde_json::json!({"cursor": "#ff0000", "folder": "#00ff00", "file": "", "mark": "nope"})));
        let base = Palette::fallback();
        let p = base.with(&o);
        assert_eq!(p.accent, Color::from_rgb8(255, 0, 0));
        assert_eq!(p.folder, Color::from_rgb8(0, 255, 0));
        assert_eq!(p.folder_mark, Some(Color::from_rgb8(0, 255, 0)));
        assert_eq!(p.file, base.file);
        assert_eq!(p.dim, base.dim);
        assert_eq!(p.selection, base.selection);
    }

    #[test]
    fn a_family_colour_draws_its_files_and_never_a_folder_or_a_link() {
        let o = Overrides::from_doc(&serde_json::json!({ "display": { "typeColors": { "image": "#00ff00" } } }));
        let p = Palette::fallback().with(&o);
        let row = |n: &str, d: bool, mode: u32| serde_json::from_value::<crate::wire::Row>(serde_json::json!({ "n": n, "d": d, "p": mode, "i": "image-x-generic" })).unwrap();
        let green = Color::from_rgb8(0, 255, 0);
        assert_eq!(p.row_ink(&row("a.png", false, 0o100644)), green);
        assert_eq!(p.row_mark(&row("a.png", false, 0o100644)), Some(green));
        assert_eq!(p.row_ink(&row("pngs", true, 0o040755)), p.folder);
        assert_eq!(p.row_ink(&row("a.png", false, 0o120777)), p.symlink);
        assert_eq!(p.row_mark(&row("a.txt", false, 0o100644)), None);
    }
}
