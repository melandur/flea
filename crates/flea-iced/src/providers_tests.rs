use super::*;
use serde_json::json;

#[test]
fn the_formats_answer_carries_the_providers_and_absent_reads_as_installed() {
    let v = json!({"t": "formats", "providers": {
        "taildrop": {"installed": false, "command": "", "reason": "tailscale is not installed."},
        "taildropSend": {"installed": true, "command": "/usr/bin/omarchy-tailscale-send", "reason": ""},
        "localsend": {"installed": true, "command": "/usr/bin/localsend-cli", "reason": ""},
        "dropbox": {"installed": true, "command": "/usr/bin/dropbox-cli", "reason": ""},
        "dropboxInfo": "{\"personal\": {\"path\": \"/home/u/Dropbox\"}}", "dropboxError": ""}});
    let p = parse(&v);
    assert!(p.known && !p.taildrop_installed && p.localsend_installed && p.dropbox_installed);
    assert_eq!((p.send_command.as_str(), p.dropbox_command.as_str()), ("/usr/bin/omarchy-tailscale-send", "/usr/bin/dropbox-cli"));
    assert_eq!(dropbox_account(p.dropbox_info.as_deref(), &p.dropbox_error), ("/home/u/Dropbox".to_string(), String::new()));
    let unknown = parse(&json!({"t": "formats"}));
    assert!(!unknown.known && unknown.taildrop_installed && unknown.dropbox_installed && !unknown.localsend_installed);
}

#[test]
fn the_dropbox_account_prefers_personal_and_refuses_a_bad_folder_by_name() {
    let both = r#"{"business": {"path": "/b"}, "personal": {"path": "/p/"}}"#;
    assert_eq!(dropbox_account(Some(both), "").0, "/p");
    assert_eq!(dropbox_account(Some(r#"{"business": {"path": "/"}}"#), "").1, "Dropbox account folder is invalid");
    assert_eq!(dropbox_account(None, "").1, "Dropbox is signed out");
    assert_eq!(dropbox_account(Some("[1]"), "").1, "Dropbox account metadata is invalid");
    assert_eq!(dropbox_account(Some(both), "unreadable").1, "unreadable");
    assert!(contains("/p", "/p/a.txt") && contains("/p", "/p") && !contains("/p", "/pq/a") && !contains("/", "/a"));
}

#[test]
fn dropbox_status_reads_the_words_because_the_exit_status_lies() {
    assert_eq!(dropbox_status("Up to date\n", Some(0), ""), "");
    assert_eq!(dropbox_status("Dropbox isn't running!", Some(0), ""), "Dropbox isn't running!");
    assert_eq!(dropbox_status("", Some(0), ""), "Dropbox returned empty status");
    assert_eq!(dropbox_status("", Some(1), "boom"), "boom");
    assert_eq!(dropbox_status("", Some(137), ""), "Dropbox status was interrupted or timed out");
}

#[test]
fn taildrop_offers_online_targets_and_names_why_when_it_offers_none() {
    let raw = json!({"BackendState": "Running", "Self": {"UserID": 7, "CapMap": {"https://tailscale.com/cap/file-sharing": []}}, "Peer": {
        "k1": {"HostName": "laptop", "DNSName": "laptop.tail.ts.net.", "Online": true, "UserID": 7},
        "k2": {"HostName": "away", "DNSName": "away.tail.ts.net.", "Online": false, "UserID": 7},
        "k3": {"HostName": "relay", "DNSName": "se-sto.mullvad.ts.net.", "Online": true, "TaildropTarget": 1},
        "k4": {"HostName": "friend", "DNSName": "friend.other.ts.net.", "Online": true, "UserID": 9},
        "k5": {"HostName": "localhost", "DNSName": "box.tail.ts.net.", "Online": true, "TaildropTarget": 1},
        "k6": {"HostName": "Alpha", "DNSName": "", "Online": true, "TaildropTarget": 1}}}).to_string();
    let (peers, reason) = taildrop_status(&raw, Some(0), "");
    let got: Vec<(&str, &str)> = peers.iter().map(|p| (p.label.as_str(), p.address.as_str())).collect();
    assert_eq!(got, [("Alpha", "Alpha"), ("box", "box.tail.ts.net"), ("laptop", "laptop.tail.ts.net")]);
    assert!(reason.is_empty());
    assert_eq!(taildrop_status(r#"{"BackendState":"NeedsLogin"}"#, Some(0), "").1, "signed out");
    assert_eq!(taildrop_status(r#"{"BackendState":"Running","Self":{}}"#, Some(0), "").1, "disabled for this account");
    assert_eq!(taildrop_status("", Some(1), "no daemon").1, "no daemon");
    assert_eq!(taildrop_status("", Some(137), "").1, "Tailscale status was interrupted or timed out");
    assert_eq!(taildrop_status("not json", Some(0), "").1, "invalid status");
}

#[test]
fn localsend_ids_pin_the_address_and_twins_are_labelled_apart() {
    let v = json!({"t": "localsendpeers", "id": 3, "peers": [
        {"name": "Clean Lemon", "address": "192.168.1.2"}, {"name": "Phone", "address": "192.168.1.3"}, {"name": "Phone", "address": "192.168.1.4"}]});
    assert_eq!(localsend_peers(&v), vec![
        ("192.168.1.2 Clean Lemon".to_string(), "Clean Lemon".to_string()),
        ("192.168.1.3 Phone".to_string(), "Phone (192.168.1.3)".to_string()),
        ("192.168.1.4 Phone".to_string(), "Phone (192.168.1.4)".to_string()),
    ]);
    assert_eq!(peer_name("192.168.1.2 Clean Lemon"), "Clean Lemon");
    assert_eq!(localsend_sending("Phone", &["/d/a.txt".into()]), "Sending a.txt to Phone with LocalSend.");
    assert_eq!(localsend_sending("Phone", &["/a".into(), "/b".into()]), "Sending 2 items to Phone with LocalSend.");
    assert_eq!(localsend_verdict(false, ""), "LocalSend could not finish the transfer.");
    assert_eq!(localsend_verdict(false, "refused"), "LocalSend · refused");
}
