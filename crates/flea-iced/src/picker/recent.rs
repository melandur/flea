// The picker's Recent location, read out of the desktop's own history and never written to: a port of
// ui/js/Recent.js plus the XBEL read ui/PickerRecent.qml left to Qt's XML reader. Every application on
// the box appends to that file, so everything here treats it as untrusted input.
use std::path::Path;

const HISTORY_LEAF: &str = "recently-used.xbel";
const DEFAULT_DATA_HOME: &str = "/.local/share";
// Bounds the output and nothing else: the whole file is read, because XBEL promises no order and a
// bound in file order would sort an arbitrary slice of it.
pub const LIMIT: usize = 500;

// An XDG_DATA_HOME that is not an absolute path is not a data home, so the default answers for it.
pub fn history_path(data_home: &str, home: &str) -> String {
    let root = if data_home.starts_with('/') { data_home.to_string() } else { format!("{home}{DEFAULT_DATA_HOME}") };
    format!("{}/{HISTORY_LEAF}", root.trim_end_matches('/'))
}

// decodeURIComponent: a malformed escape or bytes that are not UTF-8 throw there and refuse here.
fn decode(raw: &str) -> Option<String> {
    let b = raw.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'%' {
            out.push(b[i]);
            i += 1;
            continue;
        }
        let hex = |c: u8| (c as char).to_digit(16);
        let (h, l) = (hex(*b.get(i + 1)?)?, hex(*b.get(i + 2)?)?);
        out.push((h << 4 | l) as u8);
        i += 3;
    }
    String::from_utf8(out).ok()
}

// "file:///home/gm/a%20b.png" becomes "/home/gm/a b.png"; everything else becomes "". The scheme
// must be file, the authority empty or localhost, the escapes well formed, and the decoded path one
// absolute path with no control character, because each is a way a bookmark could otherwise become
// a path this window never meant to open.
pub fn path_of(href: &str) -> String {
    if href.len() < 7 || !href.is_char_boundary(7) || !href[..7].eq_ignore_ascii_case("file://") {
        return String::new();
    }
    let rest = &href[7..];
    let Some(cut) = rest.find('/') else { return String::new() };
    let authority = rest[..cut].to_ascii_lowercase();
    if !authority.is_empty() && authority != "localhost" {
        return String::new();
    }
    let Some(decoded) = decode(&rest[cut..]) else { return String::new() };
    if !decoded.starts_with('/') || decoded == "/" || decoded.chars().any(|c| (c as u32) < 0x20 || c as u32 == 0x7f) {
        return String::new();
    }
    decoded
}

// The rail's rows, newest first. Each bookmark is (href, stamp); a fixed-width UTC stamp sorts as a
// string, the file's own order breaks a tie, and a path seen twice keeps its newest position.
pub fn paths(bookmarks: &[(String, String)]) -> Vec<String> {
    let mut rows: Vec<(String, &str, usize)> = vec![];
    for (href, stamp) in bookmarks {
        let path = path_of(href);
        if !path.is_empty() {
            let at = rows.len();
            rows.push((path, stamp.as_str(), at));
        }
    }
    rows.sort_by(|a, b| b.1.cmp(a.1).then(a.2.cmp(&b.2)));
    let mut out = vec![];
    let mut seen = std::collections::HashSet::new();
    for (path, _, _) in rows {
        if out.len() >= LIMIT {
            break;
        }
        if seen.insert(path.clone()) {
            out.push(path);
        }
    }
    out
}

// /xbel/bookmark's href and its stamp: visited is when the file was last opened, which is what
// Recent means; the other two stamp the bookmark and stand in for a writer that left visited out.
// An absent, empty, unreadable or malformed history reads as no bookmarks at all.
pub fn bookmarks(body: &str) -> Vec<(String, String)> {
    let Ok(doc) = roxmltree::Document::parse(body) else { return vec![] };
    let root = doc.root_element();
    if root.tag_name().name() != "xbel" {
        return vec![];
    }
    root.children()
        .filter(|n| n.is_element() && n.tag_name().name() == "bookmark")
        .map(|n| {
            let attr = |k: &str| n.attribute(k).unwrap_or("").to_string();
            let stamp = [attr("visited"), attr("modified"), attr("added")].into_iter().find(|s| !s.is_empty()).unwrap_or_default();
            (attr("href"), stamp)
        })
        .collect()
}

pub fn read(file: &Path) -> Vec<String> {
    paths(&bookmarks(&std::fs::read_to_string(file).unwrap_or_default()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(href: &str, stamp: &str) -> (String, String) {
        (href.into(), stamp.into())
    }

    #[test]
    fn the_history_is_the_data_homes_own_file() {
        assert_eq!(history_path("/run/user/1000/data", "/home/gm"), "/run/user/1000/data/recently-used.xbel");
        assert_eq!(history_path("", "/home/gm"), "/home/gm/.local/share/recently-used.xbel");
        assert_eq!(history_path("relative/share", "/home/gm"), "/home/gm/.local/share/recently-used.xbel");
        assert_eq!(history_path("/home/gm/.local/share/", "/home/gm"), "/home/gm/.local/share/recently-used.xbel");
    }

    #[test]
    fn only_one_real_local_file_uri_becomes_a_path() {
        assert_eq!(path_of("file:///home/gm/a%20b%23c.png"), "/home/gm/a b#c.png");
        assert_eq!(path_of("file:///home/gm/caf%C3%A9.txt"), "/home/gm/café.txt");
        assert_eq!(path_of("file://localhost/etc/hostname"), "/etc/hostname");
        assert_eq!(path_of("FILE:///etc/hostname"), "/etc/hostname");
        for bad in ["file://nas.local/share/x.txt", "smb://nas/share/x.txt", "trash:///x.txt", "/home/gm/a.png", "rm -rf /home/gm", "",
            "file://", "file:///", "file:///home/gm/a%00b", "file:///home/gm/a%0Ab", "file:///home/gm/%zz", "file:///%C3"] {
            assert_eq!(path_of(bad), "", "{bad}");
        }
    }

    #[test]
    fn newest_first_once_each_and_capped_at_the_newest() {
        let read = paths(&[b("file:///home/gm/old.txt", "2026-08-01T09:00:00Z"), b("file:///home/gm/new.png", "2026-08-30T11:32:04Z"),
            b("smb://nas/share/skip.txt", "2026-08-31T11:32:04Z"), b("file:///home/gm/mid.md", "2026-08-15T10:00:00Z")]);
        assert_eq!(read, vec!["/home/gm/new.png", "/home/gm/mid.md", "/home/gm/old.txt"]);
        assert_eq!(paths(&[b("file:///a", "2026-08-30T11:32:04Z"), b("file:///a", "2026-08-01T09:00:00Z")]), vec!["/a"]);
        assert_eq!(paths(&[b("file:///first", "x"), b("file:///second", "x")]), vec!["/first", "/second"]);
        assert_eq!(paths(&[b("file:///none", ""), b("file:///dated", "2026")])[1], "/none");
        let oldest_first: Vec<_> = (0..LIMIT + 50)
            .map(|k| b(&format!("file:///home/gm/g{k}.txt"), &format!("2026-08-30T11:{}:{:02}Z", 32 + k / 60, k % 60)))
            .collect();
        let newest = paths(&oldest_first);
        assert_eq!((newest.len(), newest[0].as_str(), newest[LIMIT - 1].as_str()), (LIMIT, "/home/gm/g549.txt", "/home/gm/g50.txt"));
    }

    #[test]
    fn the_xbel_read_takes_visited_first_and_survives_garbage() {
        let body = r#"<?xml version="1.0" encoding="UTF-8"?>
<xbel version="1.0" xmlns:bookmark="http://www.freedesktop.org/standards/desktop-bookmarks">
  <bookmark href="file:///home/gm/a&amp;b.txt" added="2026-01-01T00:00:00Z" modified="2026-02-01T00:00:00Z" visited="2026-03-01T00:00:00Z"/>
  <bookmark href="file:///home/gm/c.txt" added="2026-01-01T00:00:00Z"/>
</xbel>"#;
        assert_eq!(bookmarks(body), vec![b("file:///home/gm/a&b.txt", "2026-03-01T00:00:00Z"), b("file:///home/gm/c.txt", "2026-01-01T00:00:00Z")]);
        assert!(bookmarks("<xbel><bookmark href=").is_empty());
        assert!(bookmarks("<other><bookmark href=\"file:///x\"/></other>").is_empty());
    }
}
