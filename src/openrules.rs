// The operator's own "open this ending with that application" table, and the one place its shape is
// decided. ui.json holds it as open.rules, src/uistate.rs measures a stored one against fits() and
// src/open.rs asks chosen() before it hands a file to the desktop.
//
// It exists because the desktop database keys on a sniffed type and an ending is not a type: an
// .nii and an .nii.gz are one gzip stream and one NIfTI volume to `gio open`, which is why a user
// with ITK-SNAP installed could not make the compressed one open in it without also claiming every
// other .gz on the box. So the rule is the ending, matched on the name, longest first.
use crate::jsondoc::Json;

// Long enough for ".nii.gz" and every double ending a real tool ships, short enough that a stored
// one cannot become a wall of text in the settings list.
const MAX_ENDING: usize = 32;
// The longest desktop id on this box is 58 bytes (org.freedesktop.impl.portal.desktop.flea style
// names are the long ones), so this is generous without letting a path through.
const MAX_APP: usize = 128;

// A file ending as the operator types one: a leading dot, then lowercase names and dots between
// them. Lowercase because matching is case-insensitive and the stored form is the folded one, so
// ".NII.GZ" and ".nii.gz" can never both sit in the list meaning the same thing.
pub fn is_ending(text: &str) -> bool {
    let rest = match text.strip_prefix('.') {
        Some(rest) => rest,
        None => return false,
    };
    // No empty part anywhere: a leading, trailing or doubled dot would all be one, and "..nii" is
    // the parent directory's own spelling rather than an ending.
    if rest.is_empty() || text.len() > MAX_ENDING
        || rest.starts_with('.') || rest.ends_with('.') || rest.contains("..") {
        return false;
    }
    rest.bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-' || b == b'+' || b == b'_')
}

// A desktop entry id and nothing that could be read as a path: src/open.rs makes it a path component,
// so this is the same shape crate::backend::menu_registry::resolve holds a chosen id to.
pub fn is_app(text: &str) -> bool {
    text.ends_with(".desktop")
        && text.len() > ".desktop".len()
        && text.len() <= MAX_APP
        && !text.contains('/')
        && !text.contains('\0')
        && !text.starts_with('.')
}

// One stored rule, or None for anything that is not one; the list keeps only what this answers to.
fn rule(value: &Json) -> Option<(String, String)> {
    let ends = value.get("ends").and_then(Json::as_str)?;
    let app = value.get("app").and_then(Json::as_str)?;
    (is_ending(ends) && is_app(app)).then(|| (ends.to_string(), app.to_string()))
}

// The whole key, for src/uistate.rs: an array of rules, and a file holding one bad entry keeps none
// of them rather than half a table, which is the rule every other list key in that file follows.
pub fn fits(value: &Json) -> bool {
    match value.as_array() {
        Some(items) => items.iter().all(|item| rule(item).is_some()),
        None => false,
    }
}

// The application the table names for one file NAME, not path: longest ending first, so a rule for
// ".nii.gz" answers before one for ".gz" however the two were entered, and case-insensitively, so a
// volume written out as VOLUME.NII.GZ by a scanner opens in the same place as its lowercase twin.
pub fn chosen(state: &Json, name: &str) -> Option<String> {
    let folded = name.to_ascii_lowercase();
    let rules = state.get("open").and_then(|open| open.get("rules")).and_then(Json::as_array)?;
    let mut best: Option<(usize, String)> = None;
    for item in rules {
        let (ends, app) = match rule(item) {
            Some(pair) => pair,
            None => continue,
        };
        // A name that IS the ending is a dotfile, ".nii" itself, and not a volume called nothing.
        if !folded.ends_with(&ends) || folded.len() == ends.len() {
            continue;
        }
        if best.as_ref().map(|(len, _)| ends.len() > *len).unwrap_or(true) {
            best = Some((ends.len(), app));
        }
    }
    best.map(|(_, app)| app)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jsondoc;

    fn state(rules: &str) -> Json {
        jsondoc::parse(&format!(r#"{{"open":{{"rules":{rules}}}}}"#)).expect("test state is valid JSON")
    }

    // fits() measures the key's own value, which is what src/uistate.rs hands it.
    fn key(rules: &str) -> Json {
        state(rules).get("open").and_then(|open| open.get("rules")).cloned().expect("the test state holds the key")
    }

    #[test]
    fn an_ending_is_a_dot_and_lowercase_name_parts() {
        for good in [".nii", ".nii.gz", ".mha", ".tar.gz", ".c++", ".my-notes", ".f90", ".x_y"] {
            assert!(is_ending(good), "good: {good}");
        }
        for bad in [
            "", ".", "nii", ".NII", ".nii.", ".nii..gz", "..nii", ".nii/../etc", ".nii gz",
            ".nii\0", ".012345678901234567890123456789012",
        ] {
            assert!(!is_ending(bad), "bad: {bad}");
        }
    }

    #[test]
    fn an_app_is_a_desktop_id_and_never_a_path() {
        assert!(is_app("itksnap.desktop"));
        assert!(is_app("org.gnome.Loupe.desktop"));
        for bad in ["itksnap", ".desktop", "/usr/share/applications/itksnap.desktop",
                    "../itksnap.desktop", "itksnap.desktop\0", "itksnap.desktop/x"] {
            assert!(!is_app(bad), "bad: {bad}");
        }
    }

    #[test]
    fn the_list_keeps_only_a_table_of_whole_rules() {
        assert!(fits(&key("[]")));
        assert!(fits(&key(r#"[{"ends":".nii","app":"itksnap.desktop"}]"#)));
        // One bad entry refuses the key, so the stored table is never half applied.
        assert!(!fits(&key(r#"[{"ends":".nii","app":"itksnap.desktop"},{"ends":"nii","app":"x.desktop"}]"#)));
        assert!(!fits(&key(r#"[{"ends":".nii"}]"#)));
        assert!(!fits(&key(r#"[{"ends":".nii","app":"itksnap"}]"#)));
        assert!(!fits(&key(r#"["itksnap.desktop"]"#)));
        assert!(!fits(&key("7")));
    }

    // The key through the state file itself, because that is the path the panel's write takes: one
    // bad rule in a stored table leaves the key holding the shipped default, which is no table at
    // all, rather than the half of it that happened to parse.
    #[test]
    fn a_stored_table_survives_the_file_and_a_broken_one_falls_back() {
        let kept = crate::uistate::from_file(
            r#"{"open":{"rules":[{"ends":".nii.gz","app":"itksnap.desktop"}]}}"#);
        assert_eq!(chosen(&kept, "brain.nii.gz").as_deref(), Some("itksnap.desktop"));
        for bad in [
            r#"{"open":{"rules":[{"ends":"nii","app":"itksnap.desktop"}]}}"#,
            r#"{"open":{"rules":[{"ends":".nii","app":"/bin/itksnap"}]}}"#,
            r#"{"open":{"rules":[{"ends":".nii.gz","app":"itksnap.desktop"},{"ends":".nii","app":7}]}}"#,
            r#"{"open":{"rules":"itksnap.desktop"}}"#,
        ] {
            let refused = crate::uistate::from_file(bad);
            assert_eq!(refused.get("open").and_then(|open| open.get("rules")).and_then(Json::as_array).map(<[Json]>::len),
                       Some(0), "refused: {bad}");
            assert_eq!(chosen(&refused, "brain.nii.gz"), None, "refused: {bad}");
        }
        // A patch the panel could not have built is refused outright, so the file keeps what it had.
        let current = crate::uistate::from_file(r#"{}"#);
        let patch = jsondoc::parse(r#"{"open":{"rules":[{"ends":"nii","app":"itksnap.desktop"}]}}"#).expect("valid JSON");
        assert!(crate::uistate::patched(&current, &patch).is_err());
    }

    #[test]
    fn the_longest_ending_answers_and_the_case_does_not_matter() {
        let table = state(
            r#"[{"ends":".gz","app":"file-roller.desktop"},
                {"ends":".nii.gz","app":"itksnap.desktop"},
                {"ends":".nii","app":"itksnap.desktop"}]"#,
        );
        assert_eq!(chosen(&table, "brain.nii.gz").as_deref(), Some("itksnap.desktop"));
        assert_eq!(chosen(&table, "BRAIN.NII.GZ").as_deref(), Some("itksnap.desktop"));
        assert_eq!(chosen(&table, "brain.nii").as_deref(), Some("itksnap.desktop"));
        assert_eq!(chosen(&table, "photos.tar.gz").as_deref(), Some("file-roller.desktop"));
        assert_eq!(chosen(&table, "notes.txt"), None);
        // A file whose whole name is the ending is a dotfile, and the table is about endings.
        assert_eq!(chosen(&table, ".nii"), None);
        // Nothing stored, and a table from a newer Flea that this one cannot read: no answer, and
        // src/open.rs then hands the file to the desktop exactly as it did before the key existed.
        assert_eq!(chosen(&state("[]"), "brain.nii.gz"), None);
        let broken = jsondoc::parse(r#"{"open":{"rules":{"nii":"itksnap.desktop"}}}"#).expect("valid JSON");
        assert_eq!(chosen(&broken, "brain.nii"), None);
    }
}
