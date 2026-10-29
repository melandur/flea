// Which rows get an Extract, what a new archive is called, and what the status line says while one
// runs: ui/js/Archive.js and the two archive sentences of ui/js/Ops.js. The backend never sends an
// is-archive flag, so the name is the whole mechanism, as it is in QML.

// Longest form first, so ".tar.gz" is matched before ".gz" could be. Flea reads 7z and rar and writes
// neither, so both are here, where Extract is decided, and never in the compress flyout.
const EXTENSIONS: &[&str] = &[".tar.zst", ".tar.bz2", ".tar.gz", ".tar.xz", ".tgz", ".tar", ".zip", ".7z", ".rar"];

fn matched(name: &str) -> Option<usize> {
    let lower = name.to_lowercase();
    EXTENSIONS.iter().find(|e| lower.len() > e.len() && lower.ends_with(*e)).map(|e| e.len())
}

pub fn is_archive(name: &str) -> bool {
    matched(name).is_some()
}

// The directory an extract unpacks into: the archive's own name with its archive extension off.
pub fn extract_dir(name: &str) -> String {
    match matched(name) {
        Some(n) => name[..name.len() - n].to_string(),
        None => name.to_string(),
    }
}

// One row compresses under its own name, several under the directory holding them.
pub fn archive_stem(names: &[String], parent_leaf: &str) -> String {
    if let [one] = names {
        return match one.rfind('.') {
            Some(cut) if cut > 0 => one[..cut].to_string(),
            _ => one.clone(),
        };
    }
    if parent_leaf.is_empty() { "archive".into() } else { parent_leaf.into() }
}

// Reading the last byte is not finishing, so this never says 100%; the done line says it finished.
pub fn progress_line(work: &str, done: u64, total: u64) -> String {
    if total == 0 {
        return work.to_string();
    }
    let percent = (done.saturating_mul(100) / total).min(99);
    format!("{work}, {percent}% ({} of {})", crate::format::size(done), crate::format::size(total))
}

// Ops.archiveDoneLine: an extract whose index could not be read was published unverified.
pub fn done_line(verified: bool, extracting: bool) -> &'static str {
    if !verified {
        "Extracted. The archive index could not be read, so this was not verified."
    } else if extracting {
        "Extracted."
    } else {
        "Archive written."
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_longest_extension_wins_and_a_bare_extension_is_no_archive() {
        assert!(is_archive("photos.TAR.GZ") && is_archive("a.7z") && !is_archive(".zip") && !is_archive("notes.txt"));
        assert_eq!(extract_dir("photos.tar.gz"), "photos");
        assert_eq!(extract_dir("Photos.ZIP"), "Photos");
        assert_eq!(extract_dir("plain"), "plain");
    }

    #[test]
    fn one_row_names_the_archive_and_several_take_the_folder() {
        assert_eq!(archive_stem(&["report.pdf".into()], "docs"), "report");
        assert_eq!(archive_stem(&[".bashrc".into()], "home"), ".bashrc");
        assert_eq!(archive_stem(&["a".into(), "b".into()], "docs"), "docs");
        assert_eq!(archive_stem(&["a".into(), "b".into()], ""), "archive");
    }

    #[test]
    fn progress_stops_short_of_a_hundred_and_says_nothing_without_a_total() {
        assert_eq!(progress_line("Extracting a.zip", 5, 0), "Extracting a.zip");
        assert!(progress_line("Compressing", 100, 100).starts_with("Compressing, 99% ("));
        assert!(progress_line("Compressing", 42, 100).starts_with("Compressing, 42% ("));
        assert_eq!(done_line(false, true), "Extracted. The archive index could not be read, so this was not verified.");
        assert_eq!((done_line(true, true), done_line(true, false)), ("Extracted.", "Archive written."));
    }
}
