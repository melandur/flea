// display.colors and display.typeColors, Settings > Theme > Colours: each is "" for the palette's own
// colour or exactly "#rrggbb" for the operator's. Every one ships as "", so defaults() fills both
// groups in here rather than DEFAULTS spelling out three dozen empty strings.
use crate::jsondoc::Json;
use crate::uischema::Rule;

// One role each, the colours the window draws its rows and its cursor in.
pub const COLORS: &[(&str, Rule)] = &[
    ("cursor", Rule::Colour), ("marked", Rule::Colour), ("folder", Rule::Colour), ("file", Rule::Colour),
    ("executable", Rule::Colour), ("symlink", Rule::Colour), ("mark", Rule::Colour),
];

// One per file family, named by the mark ui/js/FileTypes.js and ui/js/Icons.js give that family, so
// a colour follows exactly the files that draw that mark.
pub const TYPE_COLORS: &[(&str, Rule)] = &[
    ("image", Rule::Colour), ("film", Rule::Colour), ("music", Rule::Colour), ("archive", Rule::Colour),
    ("file-text", Rule::Colour), ("table", Rule::Colour), ("presentation", Rule::Colour), ("code", Rule::Colour),
    ("code-xml", Rule::Colour), ("braces", Rule::Colour), ("terminal", Rule::Colour), ("binary", Rule::Colour),
    ("database", Rule::Colour), ("disc", Rule::Colour), ("type", Rule::Colour), ("key", Rule::Colour),
    ("hash", Rule::Colour), ("book-open", Rule::Colour), ("palette", Rule::Colour), ("box", Rule::Colour),
    ("captions", Rule::Colour), ("mail", Rule::Colour), ("calendar", Rule::Colour), ("notebook", Rule::Colour),
    ("lock", Rule::Colour), ("git-branch", Rule::Colour),
];

pub fn is_colour(s: &str) -> bool {
    s.is_empty() || (s.len() == 7 && s.starts_with('#') && s[1..].bytes().all(|b| b.is_ascii_hexdigit()))
}

pub fn with_defaults(mut doc: Json) -> Json {
    let empty = |rules: &[(&str, Rule)]| Json::Obj(rules.iter().map(|(k, _)| (k.to_string(), Json::Str(String::new()))).collect());
    if let Json::Obj(pairs) = &mut doc {
        if let Some((_, Json::Obj(display))) = pairs.iter_mut().find(|(k, _)| k == "display") {
            display.push(("colors".into(), empty(COLORS)));
            display.push(("typeColors".into(), empty(TYPE_COLORS)));
        }
    }
    doc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_colour_is_empty_or_exactly_six_hex_digits() {
        for good in ["", "#000000", "#aBcDeF"] {
            assert!(is_colour(good), "{good:?}");
        }
        for bad in ["red", "#fff", "#1234567", "123456", "#12345g", " #123456"] {
            assert!(!is_colour(bad), "{bad:?}");
        }
    }
}
