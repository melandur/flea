// The status bar's one sentence for a backend error, ported from ui/js/Errors.js.
use crate::pane::parent;

// ui/js/Errors.js sentence, for the `where`s this front end can produce.
pub fn sentence(r#where: &str, msg: &str, path: &str) -> String {
    let named = parent(path).map(|(_, leaf)| leaf).unwrap_or_default();
    match r#where {
        "scan" if msg.to_lowercase().contains("permission denied") => {
            if named.is_empty() { "Permission denied".into() } else { format!("Permission denied on {named}") }
        }
        "scan" => {
            if named.is_empty() {
                "That directory could not be read.".into()
            } else {
                format!("That directory could not be read: {named}")
            }
        }
        "sort" => "Sorting by that column is not available.".into(),
        "read" => "The backend stopped responding; reopen Flea and try again.".into(),
        "backend" => format!("The backend is not available: {msg}"),
        "undo" | "redo" => capitalised(msg),
        "rename" if exists(msg) => "A file with that name is already here.".into(),
        "rename" => "That file could not be renamed.".into(),
        "rename-kept" => "Copied, but the old name was only partly removed. Check it.".into(),
        "mkdir" if exists(msg) => "A folder or file with that name is already here.".into(),
        "mkdir" => "That folder could not be created.".into(),
        "duplicate" => "That file could not be duplicated.".into(),
        "trash" => "That could not be moved to Trash.".into(),
        "transfer" | "archive" | "convert" => capitalised(msg),
        _ => "That action could not be completed; try again.".into(),
    }
}

fn exists(msg: &str) -> bool {
    let m = msg.to_lowercase();
    m.contains("file exists") || m.contains("already exists")
}

fn capitalised(msg: &str) -> String {
    let mut chars = msg.chars();
    let Some(first) = chars.next() else { return "That action could not be completed; try again.".into() };
    let out: String = first.to_uppercase().chain(chars).collect();
    if out.ends_with('.') { out } else { format!("{out}.") }
}
