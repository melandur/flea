// ui/js/Mounts.js for the NETWORK group: reading `gio mount -li` and `gio info` (both run under
// LC_ALL=C, AGENTS.md "Issue #36"), the bookmarks file, one canonical URI form for every comparison,
// and the line edits Remove and Rename make to ~/.config/gtk-3.0/bookmarks.
use crate::protocols::{share_name, strip_default_port};

#[derive(Debug, Clone, PartialEq)]
pub struct Mount {
    pub label: String,
    pub uri: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Bookmark {
    pub uri: String,
    pub label: String,
}

fn is_scheme_uri(s: &str) -> bool {
    let Some(i) = s.find("://") else { return false };
    let scheme = &s[..i];
    let mut chars = scheme.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c))
}

// /^Mount\(\d+\):\s*(.+?)\s*->\s*(\S+)\s*$/: the URI is the one token after the arrow, the label
// everything before it. Local, phone and camera mounts belong to the DEVICES group, not this one.
pub fn parse_mounts(output: &str) -> Vec<Mount> {
    let mut out = vec![];
    for line in output.lines() {
        let Some(rest) = line.strip_prefix("Mount(") else { continue };
        let Some(close) = rest.find("):") else { continue };
        if close == 0 || !rest[..close].bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let body = rest[close + 2..].trim();
        let Some(arrow) = body.rfind("->") else { continue };
        let uri = body[arrow + 2..].trim();
        let label = body[..arrow].trim();
        if label.is_empty() || uri.is_empty() || uri.contains(char::is_whitespace) {
            continue;
        }
        let lower = uri.to_ascii_lowercase();
        if lower.starts_with("file://") || ["mtp://", "gphoto2://", "afc://"].iter().any(|p| lower.starts_with(p)) {
            continue;
        }
        out.push(Mount { label: share_name(label, uri), uri: uri.to_string() });
    }
    out
}

// The one line of `gio info` a mount is opened by.
pub fn local_path(body: &str) -> String {
    body.lines().find_map(|l| l.strip_prefix("local path: ")).map(|p| p.trim().to_string()).unwrap_or_default()
}

pub fn leaf(uri: &str) -> String {
    let stripped = uri.find("://").filter(|_| is_scheme_uri(uri)).map(|i| &uri[i + 3..]).unwrap_or(uri);
    let stripped = stripped.trim_end_matches('/');
    let tail = stripped.rsplit('/').next().unwrap_or(stripped);
    if tail.is_empty() { stripped.to_string() } else { tail.to_string() }
}

// Mounts.nonFileBookmarks: every saved place that is not a local folder.
pub fn bookmarks(body: &str) -> Vec<Bookmark> {
    let mut out = vec![];
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("file://") {
            continue;
        }
        let (uri, label) = line.split_once(' ').map(|(u, l)| (u, l.trim())).unwrap_or((line, ""));
        if !is_scheme_uri(uri) {
            continue;
        }
        let label = if label.is_empty() { leaf(uri) } else { label.to_string() };
        out.push(Bookmark { uri: uri.to_string(), label });
    }
    out
}

// Every trailing slash stripped except a bare host root's one, and the scheme's default port dropped.
pub fn normalize(uri: &str) -> String {
    let stripped = strip_default_port(uri.trim_end_matches('/'));
    let bare_root = is_scheme_uri(&stripped) && !stripped[stripped.find("://").unwrap() + 3..].contains('/')
        && stripped.len() > stripped.find("://").unwrap() + 3;
    if bare_root { format!("{stripped}/") } else { stripped }
}

pub fn is_bare_root(uri: &str) -> bool {
    let Some(i) = uri.find("://").filter(|_| is_scheme_uri(uri)) else { return false };
    let rest = &uri[i + 3..];
    let host = rest.strip_suffix('/').unwrap_or(rest);
    !host.is_empty() && !host.contains('/')
}

// A credentialed URI names a user before the host.
pub fn credentialed(uri: &str) -> bool {
    let lower = uri.to_ascii_lowercase();
    ["smb://", "sftp://", "ftp://", "ftps://", "dav://", "davs://"].iter().any(|p| lower.starts_with(p))
        && crate::protocols::authority(uri).contains('@')
}

// A root-only remote mount covers the saved paths under it; SMB stays share-specific.
pub fn address_mount_covers(live: &str, saved: &str) -> bool {
    let (live, saved) = (normalize(live), normalize(saved));
    let scheme = crate::protocols::scheme_of(&live);
    ["sftp", "ftp", "ftps", "dav", "davs"].contains(&scheme.as_str()) && is_bare_root(&live) && saved.len() > live.len() && saved.starts_with(&live)
}

fn first_token(line: &str) -> &str {
    let t = line.trim();
    t.split_once(' ').map(|(u, _)| u).unwrap_or(t)
}

// Mounts.removeBookmark: every line naming this place goes, every other line is kept byte for byte.
pub fn remove_bookmark(body: &str, uri: &str) -> String {
    let target = normalize(uri);
    body.split('\n')
        .filter(|l| {
            let one = first_token(l);
            target.is_empty() || one.is_empty() || normalize(one) != target
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// Places.relabel: every line naming this place takes the new label, indentation dropped; a place no
// line names is appended. A newline in the name is stripped, because the file is one place a line.
pub fn relabel(body: &str, uri: &str, name: &str) -> String {
    let name: String = name.chars().filter(|c| *c != '\r' && *c != '\n').collect();
    let name = name.trim();
    if name.is_empty() {
        return body.to_string();
    }
    let target = normalize(uri);
    let mut found = false;
    let lines: Vec<String> = body
        .split('\n')
        .map(|l| {
            let one = first_token(l);
            if !l.trim().is_empty() && normalize(one) == target {
                found = true;
                format!("{one} {name}")
            } else {
                l.to_string()
            }
        })
        .collect();
    if found {
        return lines.join("\n");
    }
    let mut out = body.to_string();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    format!("{out}{target} {name}\n")
}

#[cfg(test)]
#[path = "mounts_tests.rs"]
mod tests;
