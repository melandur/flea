// The Omarchy palette, read from the same colors.toml ui/Theme.qml reads. Only the roles the
// listing pane draws are taken; an absent or unreadable file falls back to the shipped dark set.
use iced::Color;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub background: Color,
    pub surface: Color,
    pub foreground: Color,
    pub muted: Color,
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
        Palette {
            background,
            surface,
            foreground: get("foreground", d.foreground),
            muted: get("dark_foreground", get("muted", d.muted)),
            accent: get("accent", d.accent),
            selection: get("selection", d.selection),
            symlink: get("cyan", get("color6", d.symlink)),
            executable: get("green", get("color2", d.executable)),
            error: get("red", get("color1", d.error)),
        }
    }

    pub fn fallback() -> Self {
        let c = |s| hex(s).unwrap();
        Palette {
            background: c("#1a1b26"),
            surface: c("#13141c"),
            foreground: c("#a9b1d6"),
            muted: c("#565f89"),
            accent: c("#7aa2f7"),
            selection: c("#292e42"),
            symlink: c("#449dab"),
            executable: c("#9ece6a"),
            error: c("#f7768e"),
        }
    }

    pub fn load() -> Self {
        let path = std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|h| h.join(".local/state/omarchy/current/theme/colors.toml"));
        match path.and_then(|p| std::fs::read_to_string(p).ok()) {
            Some(body) => Self::from_toml(&body),
            None => Self::fallback(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_come_from_the_file_and_the_rest_fall_back() {
        let p = Palette::from_toml("accent = \"#102030\"\nbackground = \"#000000\"\n");
        assert_eq!(p.accent, Color::from_rgb8(0x10, 0x20, 0x30));
        // No dark_background, so the surface ladder takes background next.
        assert_eq!(p.surface, Color::from_rgb8(0, 0, 0));
        assert_eq!(p.error, Palette::fallback().error);
    }
}
