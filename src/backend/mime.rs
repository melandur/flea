use std::collections::HashMap;

pub struct Db {
    // A "cs" glob keys on its original text, every other glob on a lowercased copy, so one .c can beat the other.
    by_suffix_cs: HashMap<String, (u32, String)>,
    by_suffix: HashMap<String, (u32, String)>,
    // Both flagged literal names have same-type unflagged twins here, so this map changes no answer today and exists to honour the flag uniformly.
    by_name_cs: HashMap<String, (u32, String)>,
    by_name: HashMap<String, (u32, String)>,
}

const GLOBS2: &str = "/usr/share/mime/globs2";

impl Db {
    pub fn load() -> Db {
        match std::fs::read_to_string(GLOBS2) {
            Ok(text) => Db::from_str(&text),
            Err(_) => Db::from_str(""),
        }
    }

    // Sample input, /usr/share/mime/globs2: "50:image/jpeg:*.jpeg", or with the flag field, "50:text/x-csrc:*.c:cs".
    pub fn from_str(text: &str) -> Db {
        let mut by_suffix_cs: HashMap<String, (u32, String)> = HashMap::new();
        let mut by_suffix: HashMap<String, (u32, String)> = HashMap::new();
        let mut by_name_cs: HashMap<String, (u32, String)> = HashMap::new();
        let mut by_name: HashMap<String, (u32, String)> = HashMap::new();

        for line in text.lines() {
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            let mut parts = line.splitn(4, ':');
            let weight: u32 = match parts.next().and_then(|w| w.parse().ok()) {
                Some(w) => w,
                None => continue,
            };
            let mime = match parts.next() {
                Some(m) if !m.is_empty() => m,
                _ => continue,
            };
            let glob = match parts.next() {
                Some(g) if !g.is_empty() => g,
                _ => continue,
            };
            // The 4th field is a comma-separated flag list, and "cs" is the only flag this database uses.
            let cs = parts.next().is_some_and(|f| f.split(',').any(|flag| flag == "cs"));

            // corner: 10 of 1594 globs use a character class or a non-leading star; see AGENTS.md "MIME globs".
            if glob.contains('[') || glob.contains('?') {
                continue;
            }
            if let Some(suffix) = glob.strip_prefix('*') {
                if suffix.is_empty() || suffix.contains('*') {
                    continue;
                }
                let (map, key) = match cs {
                    true => (&mut by_suffix_cs, suffix.to_string()),
                    false => (&mut by_suffix, suffix.to_lowercase()),
                };
                insert_heaviest(map, key, weight, mime);
            } else if !glob.contains('*') {
                let (map, key) = match cs {
                    true => (&mut by_name_cs, glob.to_string()),
                    false => (&mut by_name, glob.to_lowercase()),
                };
                insert_heaviest(map, key, weight, mime);
            }
        }
        Db { by_suffix_cs, by_suffix, by_name_cs, by_name }
    }

    // Takes a name or a path, and answers on the last component: a directory's own dot is not a suffix.
    pub fn lookup(&self, name: &str) -> Option<&str> {
        // Issue 89, nixfred: a search or listpaths row is a path relative to the base, and the by-name
        // globs are keyed on bare names, so the component is taken here and every caller is covered.
        let name = match name.rfind('/') {
            Some(cut) => &name[cut + 1..],
            None => name,
        };
        let lower = name.to_lowercase();
        if let Some((_, mime)) = self.by_name_cs.get(name) {
            return Some(mime);
        }
        if let Some((_, mime)) = self.by_name.get(&lower) {
            return Some(mime);
        }
        // Lowercasing never adds or removes a dot, so the two strings' dots pair up and each offset indexes its own string.
        let dots = name.match_indices('.').zip(lower.match_indices('.'));
        // The earliest dot is the longest suffix, so .tar.gz beats .gz rather than racing it on weight.
        for ((dot, _), (dot_lower, _)) in dots {
            // A leading dot is a hidden file, not an extension, so .jpg must not match.
            if dot == 0 {
                continue;
            }
            if let Some((_, mime)) = self.by_suffix_cs.get(&name[dot..]) {
                return Some(mime);
            }
            if let Some((_, mime)) = self.by_suffix.get(&lower[dot_lower..]) {
                return Some(mime);
            }
        }
        None
    }
}

// The two names a sniffer emits when it declined to answer rather than when it recognised something:
// GIO answers application/x-zerosize for a file with no bytes, and xdg-mime answers inode/x-empty for
// that same file. Neither string is in /usr/share/mime/aliases, globs2 or subclasses, so Aliases::canonical
// will never pair them and no glob will ever produce one; inode/x-empty has no .xml in the database at all.
// See AGENTS.md "When the sniffer abstains".
pub fn abstained(mime: &str) -> bool {
    mime == "application/x-zerosize" || mime == "inode/x-empty"
}

// The type to answer with for one sniffed file. A sniffer that recognised something is believed, because
// content beats the name: a .txt holding PNG bytes is a PNG. Only an abstention falls back to the name's
// own glob, and only then is globs2 read, so the common path pays nothing for this.
pub fn resolved(sniffed: &str, name: &str) -> String {
    if !abstained(sniffed) {
        return sniffed.to_string();
    }
    resolved_with(&Db::load(), sniffed, name)
}

// The rule itself, split from the load so a test can put a fixture database under it rather than this
// box's own, which update-mime-database rebuilds from /usr/share/mime/packages.
pub fn resolved_with(db: &Db, sniffed: &str, name: &str) -> String {
    if !abstained(sniffed) {
        return sniffed.to_string();
    }
    match db.lookup(name) {
        Some(mime) => mime.to_string(),
        // An empty file whose name carries no glob either: nothing is known about it, and saying so
        // is what keeps the Open with guard able to refuse it.
        None => sniffed.to_string(),
    }
}

fn insert_heaviest(map: &mut HashMap<String, (u32, String)>, key: String, weight: u32, mime: &str) {
    match map.get(&key) {
        Some((existing, _)) if *existing >= weight => {}
        _ => {
            map.insert(key, (weight, mime.to_string()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Sample input, rows copied from /usr/share/mime/globs2: "weight:type:glob", with an optional 4th flag field.
    const GLOB_ROWS: &str = concat!(
        "50:image/jpeg:*.jpg\n",
        "50:image/jpeg:*.jpeg\n",
        "50:text/plain:*.txt\n",
        "50:video/mp4:*.mp4\n",
        "50:application/pdf:*.pdf\n",
        "50:application/x-compressed-tar:*.tar.gz\n",
        "50:application/gzip:*.gz\n",
        "50:text/x-makefile:makefile\n",
    );

    // A fixture database rather than this box's own, because update-mime-database merges /usr/share/mime/packages and an installed application can add or reassign a glob.
    fn db() -> Db {
        Db::from_str(GLOB_ROWS)
    }

    #[test]
    fn simple_extensions_resolve() {
        let d = db();
        assert_eq!(d.lookup("holiday.jpg"), Some("image/jpeg"));
        assert_eq!(d.lookup("notes.txt"), Some("text/plain"));
        assert_eq!(d.lookup("clip.mp4"), Some("video/mp4"));
        assert_eq!(d.lookup("paper.pdf"), Some("application/pdf"));
    }

    #[test]
    fn the_longer_extension_wins() {
        let d = db();
        assert_eq!(d.lookup("archive.tar.gz"), Some("application/x-compressed-tar"));
        assert_ne!(d.lookup("archive.tar.gz"), d.lookup("blob.gz"));
    }

    #[test]
    fn matching_ignores_case() {
        let d = db();
        assert_eq!(d.lookup("HOLIDAY.JPG"), Some("image/jpeg"));
    }

    #[test]
    fn literal_names_resolve_without_an_extension() {
        let d = db();
        assert_eq!(d.lookup("makefile"), Some("text/x-makefile"));
    }

    #[test]
    fn an_unknown_extension_is_none_not_a_guess() {
        let d = db();
        assert_eq!(d.lookup("thing.zzzznotreal"), None);
        assert_eq!(d.lookup("noextension"), None);
    }

    #[test]
    fn a_dotfile_is_not_an_extension() {
        let d = db();
        assert_eq!(d.lookup(".bashrc"), None);
        assert_eq!(d.lookup(".jpg"), None);
    }

    #[test]
    fn the_cs_flag_is_a_field_not_part_of_the_glob() {
        let text = concat!(
            "50:image/jpeg:*.jpg\n",
            "50:text/x-c++src:*.C:cs\n",
            "50:text/x-c++src:*.C\n",
            "50:text/x-csrc:*.c:cs\n",
            "50:text/x-csrc:*.c\n",
        );
        let d = Db::from_str(text);
        assert_eq!(d.lookup("foo.c"), Some("text/x-csrc"));
        assert_eq!(d.lookup("foo.C"), Some("text/x-c++src"));
        assert_eq!(d.lookup("HOLIDAY.JPG"), Some("image/jpeg"));
    }

    #[test]
    fn a_non_ascii_name_resolves_instead_of_panicking() {
        let d = db();
        assert_eq!(d.lookup("café.txt"), Some("text/plain"));
        assert_eq!(d.lookup("写真.jpg"), Some("image/jpeg"));
    }

    #[test]
    fn a_name_that_grows_when_lowercased_still_resolves() {
        let d = db();
        // "İ" lowercases to two code points, so the lowered copy is a byte longer than the name.
        assert_eq!(d.lookup("İ.txt"), Some("text/plain"));
    }

    // Issue 89, nixfred: a search row is a path relative to the base, and the by-name globs are keyed
    // on bare names, so a nested Makefile read as Data while the same file listed directly did not.
    #[test]
    fn a_row_below_the_base_is_looked_up_by_its_own_name() {
        let d = db();
        assert_eq!(d.lookup("src/Makefile"), d.lookup("Makefile"), "the by-name glob is keyed on the name");
        assert_eq!(d.lookup("src/notes.txt"), Some("text/plain"), "and a suffix below the base still resolves");
        // A dot in a directory component is not this file's extension, and a leading dot below the
        // base is a hidden file the same as one at the top, which an offset-zero test used to miss.
        assert_eq!(d.lookup("v1.2/notes"), None, "a directory's own dot offers no suffix");
        assert_eq!(d.lookup("src/.jpg"), None, "a hidden file below the base is still hidden");
        assert_eq!(d.lookup("v1.2/holiday.jpg"), Some("image/jpeg"), "and the real suffix is still read");
    }

    #[test]
    fn a_missing_database_is_empty_not_a_panic() {
        let d = Db::from_str("");
        assert_eq!(d.lookup("holiday.jpg"), None);
    }

    // The strings are the two sniffers' own sentinels and not database rows, so they are asserted
    // literally: no aliases file pairs them and no glob produces them. See AGENTS.md "When the sniffer abstains".
    #[test]
    fn only_the_two_sentinels_count_as_an_abstention() {
        assert!(abstained("application/x-zerosize"), "GIO's answer for a file with no bytes");
        assert!(abstained("inode/x-empty"), "xdg-mime's answer for the same file");
        assert!(!abstained("text/plain"));
        assert!(!abstained("inode/directory"), "a directory is a real answer, not an abstention");
        assert!(!abstained("application/octet-stream"), "unrecognised bytes are still bytes");
    }

    // Content beats the name whenever there was content to read, which is the half that must not regress:
    // a .txt holding PNG bytes is a PNG, and only an abstention may reach for the glob.
    #[test]
    fn a_sniffer_that_answered_is_believed() {
        let d = db();
        assert_eq!(resolved_with(&d, "image/png", "notes.txt"), "image/png");
        assert_eq!(resolved_with(&d, "text/plain", "notes.txt"), "text/plain");
        assert_eq!(resolved_with(&d, "inode/directory", "notes.txt"), "inode/directory");
    }

    #[test]
    fn an_abstention_falls_back_to_the_name() {
        let d = db();
        assert_eq!(resolved_with(&d, "application/x-zerosize", "empty.txt"), "text/plain");
        assert_eq!(resolved_with(&d, "inode/x-empty", "empty.txt"), "text/plain", "both sentinels take the same route");
        assert_eq!(resolved_with(&d, "application/x-zerosize", "holiday.jpg"), "image/jpeg", "the name decides, not the emptiness");
        // Nothing is known about this one, and the sentinel is kept so Open with's guard can still refuse it.
        assert_eq!(resolved_with(&d, "application/x-zerosize", "empty"), "application/x-zerosize");
        assert_eq!(resolved_with(&d, "application/x-zerosize", ".txt"), "application/x-zerosize", "a hidden file offers no suffix");
    }

    // The only test here that reads this box's own database, and it asserts that a real globs2 parses into answers, never which answer.
    #[test]
    fn the_live_database_parses_into_answers() {
        let d = Db::load();
        assert!(d.lookup("notes.txt").is_some());
        assert!(d.lookup("holiday.jpg").is_some());
        assert_eq!(d.lookup("thing.zzzznotreal"), None);
    }
}
