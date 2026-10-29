// ui/js/Protocols.js: the network schemes, their default ports, and reading a URI's parts. The port
// table is measured against gvfs 1.60.2 and glib2 2.88.3 (AGENTS.md "PR #21"): ftps is 21 there and
// not IANA's 990, and a dav:// keeps a :443 it is given.

// SCHEME_PORTS, the port gio drops when a URI names it.
pub fn default_port(scheme: &str) -> u16 {
    match scheme.to_ascii_lowercase().as_str() {
        "smb" => 445,
        "sftp" => 22,
        "ftp" | "ftps" => 21,
        "dav" => 80,
        "davs" => 443,
        "nfs" => 2049,
        _ => 0,
    }
}

pub fn scheme_of(uri: &str) -> String {
    uri.find("://").map(|i| uri[..i].to_ascii_lowercase()).unwrap_or_default()
}

// Between :// and the next /, so a ':' or '@' in the path is never a port or a host.
pub fn authority(uri: &str) -> &str {
    let Some(i) = uri.find("://") else { return "" };
    let rest = &uri[i + 3..];
    rest.find('/').map(|s| &rest[..s]).unwrap_or(rest)
}

// The userinfo dropped at its last '@', a port dropped, a bracketed IPv6 literal kept whole.
pub fn host_of(uri: &str) -> &str {
    let auth = authority(uri);
    let host_port = auth.rfind('@').map(|a| &auth[a + 1..]).unwrap_or(auth);
    if host_port.starts_with('[') {
        return host_port.find(']').map(|c| &host_port[..=c]).unwrap_or(host_port);
    }
    host_port.find(':').map(|c| &host_port[..c]).unwrap_or(host_port)
}

pub fn strip_default_port(uri: &str) -> String {
    let auth = authority(uri);
    let port = default_port(&scheme_of(uri));
    let tail = format!(":{port}");
    if auth.is_empty() || port == 0 || !auth.ends_with(&tail) {
        return uri.to_string();
    }
    let mark = uri.find("://").unwrap() + 3;
    format!("{}{}{}", &uri[..mark], &auth[..auth.len() - tail.len()], &uri[mark + auth.len()..])
}

// Protocols.shareName: gvfsd composes "<share> on <host>" in its own locale, so the tail is cut only
// when it is this URI's own host, and then exactly one connector word with it, whatever language.
pub fn share_name(raw: &str, uri: &str) -> String {
    let host = host_of(uri);
    if host.is_empty() || raw.len() <= host.len() || !raw.ends_with(host) {
        return raw.to_string();
    }
    let head = &raw[..raw.len() - host.len()];
    // head.replace(/\s+\S+\s+$/, ""): trailing whitespace, one word, whitespace before it.
    let trimmed = head.trim_end_matches(char::is_whitespace);
    if trimmed.len() == head.len() {
        return raw.to_string();
    }
    let word_start = trimmed.rfind(char::is_whitespace).map(|i| i + trimmed[i..].chars().next().unwrap().len_utf8()).unwrap_or(0);
    let before = &trimmed[..word_start];
    let name = before.trim_end_matches(char::is_whitespace);
    if word_start == 0 || name.len() == before.len() || name.is_empty() {
        return raw.to_string();
    }
    name.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_host_drops_userinfo_and_port_and_keeps_an_ipv6_literal_whole() {
        assert_eq!(host_of("sftp://user@host:2222/a@b"), "host");
        assert_eq!(host_of("smb://[fe80::1]:445/x"), "[fe80::1]");
        assert_eq!(host_of("sftp://u@h/inbox@2026"), "h");
    }

    #[test]
    fn only_the_schemes_own_default_port_is_dropped() {
        assert_eq!(strip_default_port("smb://h:445/s"), "smb://h/s");
        assert_eq!(strip_default_port("dav://h:443/x"), "dav://h:443/x");
        assert_eq!(strip_default_port("davs://h:443/x"), "davs://h/x");
        // The PR #21 trap: the path's own '@' and ':' are not the authority's.
        assert_eq!(strip_default_port("sftp://u@h:22/inbox@2026"), "sftp://u@h/inbox@2026");
        assert_eq!(strip_default_port("sftp://u@h/inbox:22"), "sftp://u@h/inbox:22");
    }

    #[test]
    fn a_share_name_cuts_its_own_host_in_any_language_and_nothing_else() {
        assert_eq!(share_name("data on nas", "smb://nas/data/"), "data");
        assert_eq!(share_name("stubshare en stubhost", "smb://stubhost/stubshare/"), "stubshare");
        // AGENTS.md issue #36: a label whose tail is not this host is not touched.
        assert_eq!(share_name("backup on tuesday", "smb://nas/backup/"), "backup on tuesday");
        assert_eq!(share_name("backup on tuesday on nas", "smb://nas/backup/"), "backup on tuesday");
        assert_eq!(share_name("Pixel 7", "mtp://Google_Pixel_7_1A2B/"), "Pixel 7");
        assert_eq!(share_name("x on fe80::1", "smb://[fe80::1]/x"), "x on fe80::1");
    }
}
