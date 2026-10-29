// ui/js/Kinds.js: what the preview draws a row as. The name decides PDF, Markdown and the tables
// before the icon does; everything else follows the icon name the backend already sent.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Image,
    Video,
    Audio,
    Pdf,
    Text,
    Archive,
    Unsupported,
}

fn ends_with_any(name: &str, exts: &[&str]) -> bool {
    let lower = name.to_lowercase();
    exts.iter().any(|e| lower.len() > e.len() && lower.ends_with(e))
}

pub fn is_markdown(name: &str) -> bool {
    ends_with_any(name, &[".md", ".markdown", ".mkd"])
}

pub fn is_delimited(name: &str) -> bool {
    ends_with_any(name, &[".csv", ".tsv", ".tab", ".psv"])
}

pub fn is_sheet(name: &str) -> bool {
    ends_with_any(name, &[".xlsx", ".xlsm", ".xltx", ".xltm", ".ods", ".ots", ".fods"])
}

pub fn is_tabular(name: &str) -> bool {
    is_delimited(name) || is_sheet(name)
}

pub fn is_pdf(name: &str) -> bool {
    ends_with_any(name, &[".pdf"])
}

// Kinds.kindState: code reads as text here, which is what quickLookKind does with it.
fn by_icon(icon: &str) -> Kind {
    match icon {
        "text-x-script" | "text-html" | "application-xml" => Kind::Text,
        "package-x-generic" => Kind::Archive,
        _ if icon.starts_with("image-") => Kind::Image,
        _ if icon.starts_with("video-") => Kind::Video,
        _ if icon.starts_with("audio-") => Kind::Audio,
        _ if icon.starts_with("text-") => Kind::Text,
        _ => Kind::Unsupported,
    }
}

pub fn quick_look(icon: &str, name: &str) -> Kind {
    if is_pdf(name) {
        return Kind::Pdf;
    }
    if is_markdown(name) || is_tabular(name) {
        return Kind::Text;
    }
    by_icon(icon)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_name_decides_before_the_icon() {
        assert_eq!(quick_look("x-office-document", "a.PDF"), Kind::Pdf);
        assert_eq!(quick_look("x-office-document", "notes.md"), Kind::Text);
        assert_eq!(quick_look("x-office-spreadsheet", "s.xlsx"), Kind::Text);
        assert_eq!(quick_look("package-x-generic", "brain.nii.gz"), Kind::Archive);
        assert_eq!(quick_look("text-x-script", "run.sh"), Kind::Text);
        assert_eq!(quick_look("image-x-generic", "a.png"), Kind::Image);
        assert_eq!(quick_look("application-x-executable", "a.out"), Kind::Unsupported);
        // A name that is only the ending is a dotfile, not that ending's file.
        assert_eq!(quick_look("text-x-generic", ".md"), Kind::Text);
        assert!(!is_markdown(".md"));
    }
}
