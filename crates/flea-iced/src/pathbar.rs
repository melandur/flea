// What a typed path line means and what the crumbs above the listing are: ui/js/PathBar.js and
// ui/js/Crumbs.js. Pure, as those two files were, so every rule here is a test and not a window.

// PathBar.normalize: empty and "." parts dropped, ".." pops, one leading slash.
pub fn normalize(path: &str) -> String {
    if path.is_empty() {
        return String::new();
    }
    let mut out: Vec<&str> = vec![];
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            p => out.push(p),
        }
    }
    format!("/{}", out.join("/"))
}

// PathBar.unwrap: a file:// URI on this machine becomes its decoded path, one on another host
// becomes "", and anything else passes through unchanged.
pub fn unwrap(text: &str) -> String {
    let Some(body) = text.strip_prefix("file://") else { return text.to_string() };
    let (authority, rest) = match body.find('/') {
        Some(cut) => (&body[..cut], &body[cut..]),
        None => (body, "/"),
    };
    if !authority.is_empty() && !authority.eq_ignore_ascii_case("localhost") {
        return String::new();
    }
    crate::rail::decode_path(rest)
}

// PathBar.resolve: the tilde, a relative name against the folder on screen, and the interior . and ..
pub fn resolve(text: &str, current: &str, home: &str) -> String {
    let body = unwrap(text.trim());
    if body.is_empty() {
        return String::new();
    }
    let body = if !home.is_empty() && (body == "~" || body.starts_with("~/")) {
        format!("{home}{}", &body[1..])
    } else if !body.starts_with('/') {
        format!("{current}/{body}")
    } else {
        body
    };
    normalize(&body)
}

// A line that named something and still resolved to nothing: a file:// URI on another host.
pub fn refused(text: &str) -> bool {
    let line = text.trim();
    !line.is_empty() && unwrap(line).is_empty()
}

fn split(text: &str) -> (&str, &str) {
    match text.rfind('/') {
        Some(cut) => (&text[..=cut], &text[cut + 1..]),
        None => ("", text),
    }
}

// Which folder Tab lists: the typed line's head, or the folder on screen when there is none.
pub fn completion_dir(text: &str, current: &str, home: &str) -> String {
    let (head, _) = split(text.trim());
    if head.is_empty() { current.to_string() } else { resolve(head, current, home) }
}

// A leaf starting with a dot asks for the dotfiles whatever the setting says.
pub fn wants_hidden(text: &str, show_hidden: bool) -> bool {
    show_hidden || split(text.trim()).1.starts_with('.')
}

// PathBar.complete: the names the leaf starts, case-sensitively first and folded only when that
// finds none; the line grows to their common prefix, and a single match gains its slash.
pub fn complete(text: &str, names: &[String]) -> (String, usize) {
    let (head, leaf) = split(text);
    let mut matched: Vec<&String> = names.iter().filter(|n| n.starts_with(leaf)).collect();
    if matched.is_empty() {
        let low = leaf.to_lowercase();
        matched = names.iter().filter(|n| n.to_lowercase().starts_with(&low)).collect();
    }
    if matched.is_empty() {
        return (text.to_string(), 0);
    }
    let mut prefix: String = matched[0].clone();
    for n in &matched[1..] {
        let common = prefix.chars().zip(n.chars()).take_while(|(a, b)| a == b).count();
        prefix = prefix.chars().take(common).collect();
    }
    // A folded match can share a shorter prefix than the leaf typed; the leaf is never shortened.
    if prefix.chars().count() < leaf.chars().count() {
        prefix = leaf.to_string();
    }
    let tail = if matched.len() == 1 { format!("{prefix}/") } else { prefix };
    (format!("{head}{tail}"), matched.len())
}

pub fn completion_message(before: &str, after: &(String, usize), dir: &str) -> Option<String> {
    if after.1 == 0 {
        return Some(format!("Nothing in {dir} starts with that."));
    }
    (after.0 == before).then(|| format!("{} names share that prefix.", after.1))
}

#[derive(Debug, Clone, PartialEq)]
pub struct Crumb {
    pub text: String,
    pub path: String,
    pub last: bool,
}

// Crumbs.crumbs: "~" and "/" are crumbs of their own, every crumb with another after it carries a
// separator, and the last one, the folder already listed, answers no single click.
pub fn crumbs(path: &str, home: &str) -> Vec<Crumb> {
    let in_home = !home.is_empty() && (path == home || path.starts_with(&format!("{home}/")));
    let display = if in_home { format!("~{}", &path[home.len()..]) } else { path.to_string() };
    let parts: Vec<&str> = display.split('/').collect();
    let mut walked = if in_home { home.to_string() } else { String::new() };
    let first = if parts.len() > 1 { format!("{}/", parts[0]) } else { parts[0].to_string() };
    let mut out = vec![Crumb { text: first, path: if walked.is_empty() { "/".into() } else { walked.clone() }, last: false }];
    for part in parts.iter().skip(1).filter(|p| !p.is_empty()) {
        walked = format!("{walked}/{part}");
        out.push(Crumb { text: format!("{part}/"), path: walked.clone(), last: false });
    }
    let many = out.len() > 1;
    let end = out.last_mut().unwrap();
    if many {
        end.text.pop();
    }
    end.last = true;
    out
}

#[cfg(test)]
#[path = "pathbar_tests.rs"]
mod tests;
