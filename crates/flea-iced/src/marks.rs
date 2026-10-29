// The row mark: ui/js/Icons.js picks it and ui/Glyph.qml draws it. Lucide on a 24-unit grid at
// stroke 2, the Omarchy cut of square caps and mitered joins, tinted with one palette colour. The
// paths, families and colours come from tables build.rs generates; see that file's header.
use crate::format::is_symlink;
use iced::widget::svg;
use iced::Color;
use std::cell::RefCell;
use std::collections::HashMap;

mod tables {
    include!(concat!(env!("OUT_DIR"), "/tables.rs"));
}

fn find(table: &'static [(&'static str, &'static str)], key: &str) -> Option<&'static str> {
    table.binary_search_by(|(k, _)| (*k).cmp(key)).ok().map(|i| table[i].1)
}

pub const FALLBACK: &str = "file";

// Icons.js GLYPHS: the backend's freedesktop icon name to a mark.
pub fn glyph_for(icon: &str) -> &'static str {
    match icon {
        "folder" | "inode-directory" => "folder",
        "text-x-generic" | "text-plain" | "x-office-document" => "file-text",
        "text-x-script" | "text-html" | "application-xml" => "code",
        "image-x-generic" => "image",
        "video-x-generic" => "film",
        "audio-x-generic" => "music",
        "package-x-generic" => "archive",
        "font-x-generic" => "type",
        "application-x-executable" => "terminal",
        "x-office-spreadsheet" => "table",
        "x-office-presentation" => "presentation",
        _ => FALLBACK,
    }
}

// FileTypes.extensions: every suffix after a dot, longest first; a dotfile's own dot is not one.
fn extensions(lower: &str) -> impl Iterator<Item = &str> {
    let bytes = lower.as_bytes();
    (1..bytes.len().saturating_sub(1)).filter(move |&i| bytes[i] == b'.').map(move |i| &lower[i + 1..])
}

// FileTypes.markFor: an exact name first, then the longest extension a family claims.
pub fn mark_for(name: &str, is_dir: bool) -> Option<&'static str> {
    if is_dir {
        return None;
    }
    let lower = name.to_lowercase();
    find(tables::NAME_MARKS, &lower).or_else(|| extensions(&lower).find_map(|e| find(tables::EXT_MARKS, e)))
}

// Icons.glyphForRow: a link keeps the link mark whatever it points at; the filetype tier only when on.
pub fn glyph_for_row(icon: &str, mode: u32, name: &str, is_dir: bool, file_types: bool) -> &'static str {
    if is_symlink(mode) {
        return "symlink";
    }
    if file_types {
        if let Some(m) = mark_for(name, is_dir) {
            return m;
        }
    }
    glyph_for(icon)
}

fn hex(s: &str) -> Option<Color> {
    let n = u32::from_str_radix(s.strip_prefix('#')?, 16).ok()?;
    Some(Color::from_rgb8((n >> 16) as u8, (n >> 8) as u8, n as u8))
}

// FileTypes.rowColor: a directory reads the dirs table, a file its exact name then its extensions;
// a symlink takes the theme's own colour and never a vendored one.
pub fn row_colour(name: &str, mode: u32, is_dir: bool) -> Option<Color> {
    if is_symlink(mode) {
        return None;
    }
    let lower = name.to_lowercase();
    let found = if is_dir {
        find(tables::DIR_COLOURS, &lower)
    } else {
        find(tables::NAME_COLOURS, &lower).or_else(|| extensions(&lower).find_map(|e| find(tables::EXT_COLOURS, e)))
    };
    found.and_then(hex)
}

pub fn path_for(mark: &str) -> &'static str {
    find(tables::PATHS, mark).or_else(|| find(tables::PATHS, FALLBACK)).unwrap_or("")
}

thread_local! {
    // One handle per mark and colour, so iced's raster cache keys on the same handle every frame:
    // a new handle each frame would rasterise the same mark again, row after row.
    static HANDLES: RefCell<HashMap<(&'static str, [u8; 4]), svg::Handle>> = RefCell::new(HashMap::new());
}

pub fn handle(mark: &'static str, colour: Color) -> svg::Handle {
    let rgba = colour.into_rgba8();
    HANDLES.with(|h| {
        h.borrow_mut()
            .entry((mark, rgba))
            .or_insert_with(|| {
                let [r, g, b, _] = rgba;
                let body = format!(
                    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#{r:02x}{g:02x}{b:02x}" stroke-width="2" stroke-linecap="square" stroke-linejoin="miter"><path d="{}"/></svg>"##,
                    path_for(mark)
                );
                svg::Handle::from_memory(body.into_bytes())
            })
            .clone()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_longest_extension_wins_and_a_dotfile_is_a_name() {
        // d.ts is a compound key in the table; ts alone is code as well, so test via the name path.
        assert_eq!(extensions("a.d.ts").collect::<Vec<_>>(), vec!["d.ts", "ts"]);
        assert_eq!(extensions(".bashrc").count(), 0);
        assert_eq!(mark_for(".bashrc", false), Some("terminal"));
        assert_eq!(mark_for("Cargo.lock", false), Some("lock"));
        assert_eq!(mark_for("photo.JPG", false), Some("image"));
        assert_eq!(mark_for("brain.nii.gz", false), Some("archive"));
        assert_eq!(mark_for("anything", true), None);
    }

    #[test]
    fn a_link_keeps_the_link_mark_and_the_tier_only_applies_when_on() {
        assert_eq!(glyph_for_row("text-x-generic", 0o120777, "a.rs", false, true), "symlink");
        assert_eq!(glyph_for_row("text-x-generic", 0o100644, "a.rs", false, false), "file-text");
        assert_eq!(glyph_for_row("text-x-generic", 0o100644, "a.rs", false, true), "code");
    }

    #[test]
    fn every_mark_draws_something_other_than_the_file_mark() {
        for mark in ["folder", "file-text", "code", "image", "film", "music", "archive", "type", "terminal", "table", "presentation", "symlink", "lock", "braces"] {
            assert!(!path_for(mark).is_empty());
            assert!(mark == FALLBACK || path_for(mark) != path_for(FALLBACK), "{mark} falls back to the file mark");
        }
    }

    #[test]
    fn colours_follow_the_vendored_table() {
        // go is #00add8 in vendor/yazi-icons.toml; Downloads is one of the #00bcd4 directories.
        assert_eq!(row_colour("main.go", 0o100644, false), hex("#00add8"));
        assert_eq!(row_colour("Downloads", 0o040755, true), hex("#00bcd4"));
        assert_eq!(row_colour("main.go", 0o120777, false), None);
    }
}
