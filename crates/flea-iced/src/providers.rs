// What the send rows know about the three services behind them, as ui/js/Taildrop.js, ui/js/Dropbox.js
// and ui/js/LocalSend.js read it: the providers block the backend's `formats` answer carries, the
// Dropbox account folder and status, `tailscale status --json`'s peers, and LocalSend's devices and
// sentences. Toolkit free; sendshell.rs runs the helpers and carries the answers.
use serde_json::Value;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Providers {
    // Absent until the first `formats` answer, which Menu.js reads as installed, so a row is not
    // dropped from a menu raised before the probe answered.
    pub known: bool,
    pub taildrop_installed: bool,
    pub taildrop_command: String,
    pub taildrop_reason: String,
    pub send_command: String,
    pub localsend_installed: bool,
    pub localsend_reason: String,
    pub dropbox_installed: bool,
    pub dropbox_command: String,
    pub dropbox_reason: String,
    pub dropbox_info: Option<String>,
    pub dropbox_error: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Peer {
    pub id: String,
    pub label: String,
    pub address: String,
}

pub fn parse(formats: &Value) -> Providers {
    let p = |k: &str| formats.pointer(&format!("/providers/{k}")).cloned().unwrap_or(Value::Null);
    let s = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let installed = |v: &Value| v.get("installed").and_then(Value::as_bool) != Some(false);
    let (taildrop, send, localsend, dropbox) = (p("taildrop"), p("taildropSend"), p("localsend"), p("dropbox"));
    Providers {
        known: formats.get("providers").is_some(),
        taildrop_installed: installed(&taildrop),
        taildrop_command: s(&taildrop, "command"),
        taildrop_reason: s(&taildrop, "reason"),
        send_command: s(&send, "command"),
        // LocalSend's row is offered only on an explicit yes (Pane.qml's `installed === true`).
        localsend_installed: localsend.get("installed").and_then(Value::as_bool) == Some(true),
        localsend_reason: s(&localsend, "reason"),
        dropbox_installed: installed(&dropbox),
        dropbox_command: s(&dropbox, "command"),
        dropbox_reason: s(&dropbox, "reason"),
        dropbox_info: formats.pointer("/providers/dropboxInfo").and_then(Value::as_str).map(String::from),
        dropbox_error: formats.pointer("/providers/dropboxError").and_then(Value::as_str).unwrap_or("").to_string(),
    }
}

// Dropbox.account: personal before business, and a folder that is not an absolute non-root path is
// refused by name. Answers (path, reason), exactly one of them non-empty.
pub fn dropbox_account(raw: Option<&str>, error: &str) -> (String, String) {
    if !error.is_empty() {
        return (String::new(), error.into());
    }
    let Some(raw) = raw.filter(|r| !r.is_empty()) else { return (String::new(), "Dropbox is signed out".into()) };
    let Ok(Value::Object(data)) = serde_json::from_str::<Value>(raw) else { return (String::new(), "Dropbox account metadata is invalid".into()) };
    for key in ["personal", "business"] {
        let Some(Value::Object(account)) = data.get(key) else { continue };
        return match account.get("path").and_then(Value::as_str) {
            Some(path) if path.starts_with('/') && path != "/" && !path.contains('\0') => (path.trim_end_matches('/').to_string(), String::new()),
            _ => (String::new(), "Dropbox account folder is invalid".into()),
        };
    }
    (String::new(), "Dropbox is signed out".into())
}

pub fn contains(root: &str, path: &str) -> bool {
    root.len() > 1 && root.starts_with('/') && (path == root || path.starts_with(&format!("{root}/")))
}

// Dropbox.status: dropbox-cli says "Dropbox isn't running!" with exit status zero, so the text decides.
pub fn dropbox_status(output: &str, code: Option<i32>, error: &str) -> String {
    let (text, detail) = (output.trim(), error.trim());
    if matches!(code, Some(137) | Some(9) | None) {
        return "Dropbox status was interrupted or timed out".into();
    }
    if code != Some(0) {
        return [detail, text, "Dropbox status failed"].into_iter().find(|s| !s.is_empty()).unwrap().into();
    }
    if text.is_empty() {
        return if detail.is_empty() { "Dropbox returned empty status".into() } else { detail.into() };
    }
    if !detail.is_empty() {
        return detail.into();
    }
    let lower = text.to_lowercase();
    let stopped = lower.contains("not running") || lower.contains("isn't running") || lower == "stopped"
        || text.starts_with("Couldn't get status:") || text == "Dropbox isn't responding!" || text == "Dropbox daemon stopped.";
    if stopped { text.into() } else { String::new() }
}

fn clean_dns(name: &str) -> &str {
    name.strip_suffix('.').unwrap_or(name)
}

// Taildrop.status: the backend state, the file-sharing capability, then the online peers that are
// Taildrop targets and not Mullvad relays, by label. Answers (peers, reason).
pub fn taildrop_status(raw: &str, code: Option<i32>, error: &str) -> (Vec<Peer>, String) {
    if code != Some(0) {
        let why = if matches!(code, Some(137) | Some(9)) { "Tailscale status was interrupted or timed out".to_string() } else { error.trim().to_string() };
        return (vec![], if why.is_empty() { "Tailscale status failed.".into() } else { why });
    }
    let Ok(data) = serde_json::from_str::<Value>(raw) else { return (vec![], "invalid status".into()) };
    if !data.is_object() {
        return (vec![], "invalid status".into());
    }
    match data.get("BackendState").and_then(Value::as_str) {
        Some("Running") => {}
        Some("NeedsLogin") => return (vec![], "signed out".into()),
        other => return (vec![], other.unwrap_or("unavailable").to_lowercase()),
    }
    let me = &data["Self"];
    let cap = "https://tailscale.com/cap/file-sharing";
    let listed = me["Capabilities"].as_array().is_some_and(|c| c.iter().any(|x| x == cap));
    if me.pointer("/CapMap").and_then(|m| m.get(cap)).is_none() && !listed {
        return (vec![], "disabled for this account".into());
    }
    let self_user = me["UserID"].to_string();
    let mut peers = vec![];
    for (id, peer) in data["Peer"].as_object().into_iter().flatten() {
        let dns = clean_dns(peer["DNSName"].as_str().unwrap_or(""));
        let host = peer["HostName"].as_str().unwrap_or("");
        let mullvad = |n: &str| n.to_lowercase().ends_with(".mullvad.ts.net") && n.len() > ".mullvad.ts.net".len();
        if peer["Online"] != Value::Bool(true) || mullvad(dns) || mullvad(host) {
            continue;
        }
        let target = match peer["TaildropTarget"].as_i64() {
            Some(t) if t != 0 => t == 1,
            _ => !peer["UserID"].is_null() && peer["UserID"].to_string() == self_user,
        };
        let address = if !dns.is_empty() {
            dns.to_string()
        } else if !host.is_empty() {
            host.to_string()
        } else {
            peer["TailscaleIPs"].as_array().into_iter().flatten().filter_map(Value::as_str).find(|ip| ip.starts_with("100.")).unwrap_or("").to_string()
        };
        if !target || address.is_empty() {
            continue;
        }
        let label = if !host.is_empty() && host.to_lowercase() != "localhost" {
            host.to_string()
        } else {
            let short = dns.split('.').next().unwrap_or("");
            [short, host, "Unknown"].into_iter().find(|s| !s.is_empty()).unwrap().to_string()
        };
        peers.push(Peer { id: id.clone(), label, address });
    }
    peers.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()));
    let reason = if peers.is_empty() { "no peers".to_string() } else { String::new() };
    (peers, reason)
}

// LocalSend.answered: an id is "<address> <name>", so a second device with the same name cannot
// take the files, and two of one name are labelled apart by their address.
pub fn localsend_peers(v: &Value) -> Vec<(String, String)> {
    let list: Vec<(String, String)> = v["peers"].as_array().into_iter().flatten()
        .map(|p| (p["name"].as_str().unwrap_or("").to_string(), p["address"].as_str().unwrap_or("").to_string()))
        .collect();
    list.iter().map(|(name, address)| {
        let twins = list.iter().filter(|(n, _)| n == name).count() > 1;
        (format!("{address} {name}"), if twins { format!("{name} ({address})") } else { name.clone() })
    }).collect()
}

pub fn peer_name(id: &str) -> &str {
    id.split_once(' ').map(|(_, n)| n).unwrap_or(id)
}

pub fn localsend_sending(peer: &str, paths: &[String]) -> String {
    match paths {
        [one] => format!("Sending {} to {peer} with LocalSend.", one.rsplit('/').next().unwrap_or(one)),
        _ => format!("Sending {} items to {peer} with LocalSend.", paths.len()),
    }
}

pub fn localsend_verdict(ok: bool, reason: &str) -> String {
    if ok {
        "LocalSend finished the transfer.".into()
    } else if reason.is_empty() {
        "LocalSend could not finish the transfer.".into()
    } else {
        format!("LocalSend · {reason}")
    }
}

#[cfg(test)]
#[path = "providers_tests.rs"]
mod tests;
