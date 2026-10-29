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

// Errors.js VERIFY: the prefix the dialog reads a question by, the way it reads a failure by
// "Connect failed:". A question is not a failure: the password was never tried.
pub const VERIFY: &str = "Verify: ";

pub fn is_question(message: &str) -> bool {
    message.starts_with(VERIFY)
}

// Errors.hostOf: the host a sentence names instead of quoting the whole address.
fn host_named(uri: &str) -> String {
    let h = crate::protocols::host_of(uri);
    let h = h.split(':').next().unwrap_or(h);
    if h.is_empty() { "that server".into() } else { h.to_string() }
}

// Errors.identityQuestion: what tools/flea-gio-auth printed when it refused gvfs's identity prompt,
// as one sentence with the fingerprint in it. changed is exit 5, a key that is not the one accepted.
pub fn identity_question(asked: &str, changed: bool, uri: &str) -> String {
    let key = fingerprint(asked).unwrap_or_else(|| "an unreadable key".into());
    let host = host_named(uri);
    if changed {
        return format!(
            "{VERIFY}{host} answers with a DIFFERENT key than the one this computer accepted before. It now offers {key}. A rebuilt server does this; so does something pretending to be it. Connect anyway?"
        );
    }
    format!("{VERIFY}{host} has not been connected to from this computer before. It offers {key}. Connect anyway?")
}

// SHA256:..., SHA1:... or MD5:..., else six or more colon-separated hex pairs.
fn fingerprint(text: &str) -> Option<String> {
    // The regex's own order: the earliest position in the text wins, whichever algorithm it names.
    let mut hits: Vec<(usize, &str)> = ["SHA256:", "SHA1:", "MD5:"]
        .iter()
        .flat_map(|algo| text.match_indices(*algo).map(move |(i, _)| (i, *algo)))
        .collect();
    hits.sort();
    for (i, algo) in hits {
        let tail: String = text[i + algo.len()..].chars().take_while(|c| c.is_ascii_alphanumeric() || "+/=".contains(*c)).collect();
        if !tail.is_empty() {
            return Some(format!("{algo}{tail}"));
        }
    }
    text.split(|c: char| !(c.is_ascii_hexdigit() || c == ':'))
        .find(|w| {
            let parts: Vec<&str> = w.split(':').collect();
            parts.len() >= 6 && parts.iter().all(|p| p.len() == 2)
        })
        .map(str::to_string)
}

// Errors.connectFailure: the helper's exit code as one sentence. 3 and 4 are the server not being
// vouched for and 6 a gio that never asked for a password; none of those is a refused password.
pub fn connect_failure(code: Option<i32>, uri: &str) -> String {
    match code {
        Some(124) => "Connect failed: host did not respond".into(),
        Some(126 | 127) => "Connect failed: authentication helper is unavailable".into(),
        Some(3) => "Connect failed: this server's identity is not known. Connect to it once in a terminal to check and accept its key, then retry.".into(),
        Some(4) => "Connect failed: this server's certificate is not trusted.".into(),
        Some(6) => "Connect failed: the server refused the connection before asking for a password".into(),
        _ if ["ftp:", "ftps:", "dav:", "davs:"].iter().any(|p| uri.to_ascii_lowercase().starts_with(p)) => {
            "Connect failed: host refused the TLS handshake".into()
        }
        _ => "Connect failed: authentication was refused".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_key_question_carries_the_fingerprint_and_a_changed_key_says_so_first() {
        let asked = "The identity of the remote computer (submit02.unibe.ch) is unknown.\nThis happens when you log into a computer the first time.\n\nThe identity sent by the remote computer is SHA256:abc+/9=. If you want to be absolutely sure...\n[1] Log In Anyway\n[2] Cancel Login\nChoice: ";
        let q = identity_question(asked, false, "sftp://u@submit02.unibe.ch/x");
        assert_eq!(q, "Verify: submit02.unibe.ch has not been connected to from this computer before. It offers SHA256:abc+/9=. Connect anyway?");
        assert!(is_question(&q));
        assert!(identity_question("aa:bb:cc:dd:ee:ff changed", true, "sftp://h/").contains("DIFFERENT key"));
        assert!(identity_question("nothing", false, "sftp://h/").contains("an unreadable key"));
    }

    #[test]
    fn the_helpers_codes_never_blame_a_password_that_was_not_tried() {
        assert!(connect_failure(Some(3), "sftp://h/").contains("identity is not known"));
        assert_eq!(connect_failure(Some(1), "sftp://h/"), "Connect failed: authentication was refused");
        assert_eq!(connect_failure(Some(1), "davs://h/"), "Connect failed: host refused the TLS handshake");
        assert_eq!(connect_failure(None, "smb://h/"), "Connect failed: authentication was refused");
    }
}
