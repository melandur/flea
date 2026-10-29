// The NETWORK group's service, ui/NetworkMounts.qml and ui/MountListing.qml with no toolkit in them.
//
// Opening a place is up to three legs, each bounded at 15 s and only ever one at a time: `gio mount`,
// then `gio info` for the FUSE path, then for a bare server root `gio list` for its shares. The mount
// leg's exit code is never the verdict: gvfsd's "already mounted" refusal is translated and exits 2
// on a share that is perfectly openable, so `gio info` decides every time (AGENTS.md "Issue #36").
// The open itself, signing in included, is netopen.rs; this file is the listing and the rows.
use crate::mounts::{self, Bookmark};
use std::collections::{HashMap, HashSet};
use crate::process::Ran;

pub const POLL_MS: u64 = 5000;
pub const LIST_TIMEOUT_MS: u64 = 10000;
pub const LEG_TIMEOUT_MS: u64 = 15000;

#[derive(Debug, Clone, PartialEq)]
pub struct Share {
    pub label: String,
    pub uri: String,
    // The live mount that covers a saved deeper path, which is what an unmount names.
    pub mount_uri: Option<String>,
    pub mounted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Leg {
    Mount,
    Info,
    List,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    List,
    // Every leg carries the open's generation, so a result that outlived a cancel answers nothing.
    Leg(u64, Leg, Vec<String>),
    // tools/flea-gio-auth with the password on stdin, never in its argv.
    Auth(u64, Vec<String>, String),
    Unmount(Vec<String>),
    Message(String, bool),
    Opened(String),
    SharesListed { base: String, label: String, names: Vec<String> },
    // The dialog's own request finished; it reads success, or the reason, and decides what to show.
    Completed { id: String, uri: String, success: bool, reason: String },
    // retryRequested: open the dialog on this place with the reason in its status line.
    AskPassword { uri: String, label: String, password: String, reason: String, failed: bool },
}

#[derive(Debug)]
pub(crate) struct Opening {
    pub(crate) uri: String,
    pub(crate) label: String,
    pub(crate) mount_failed: bool,
    // The dialog request this open answers, and the password it came with.
    pub(crate) request: Option<(String, String)>,
    // A helper refusal held until gio info has had its say: the sentence, and whether the
    // password itself was refused and should be forgotten.
    pub(crate) auth_failure: Option<(String, bool)>,
}

#[derive(Debug, Default)]
pub struct Network {
    pub entries: Vec<Share>,
    pub remember_passwords: bool,
    listing_text: String,
    bookmarks_text: String,
    listing: bool,
    poll_again: bool,
    pub(crate) opening: Option<Opening>,
    pub(crate) leg_running: bool,
    pub(crate) generation: u64,
    // The place the last open was for, kept after it ends: the share browser's children carry its
    // password, the way NetworkMounts' _pendingUri outlives the open it named.
    pub(crate) opening_uri: Option<String>,
    // Per window and never written anywhere: what stops one place asking twice in a session.
    pub(crate) passwords: HashMap<String, String>,
    // The operator's yes to a host key, spent by the attempt it was given for.
    pub(crate) trusted: HashSet<String>,
    unmount_label: Option<String>,
}

pub(crate) fn argv(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| s.to_string()).collect()
}

impl Network {
    pub fn new(remember_passwords: bool) -> Self {
        Network { remember_passwords, ..Default::default() }
    }

    // A re-read asked for while one is out runs when it ends, rather than being dropped.
    pub fn poll(&mut self) -> Vec<Effect> {
        if self.listing {
            self.poll_again = true;
            return vec![];
        }
        self.listing = true;
        vec![Effect::List]
    }

    // A listing that timed out or failed collected nothing, and reading that as "no shares" would
    // empty the rail and take Unmount with it exactly when a server misbehaves, so the rows stand.
    pub fn listed(&mut self, ran: Ran) -> Vec<Effect> {
        self.listing = false;
        if let Ran::Exited(Some(0), text) = ran {
            self.listing_text = text;
            self.rebuild();
        }
        if std::mem::take(&mut self.poll_again) {
            return self.poll();
        }
        vec![]
    }

    pub fn set_bookmarks(&mut self, text: String) {
        if text != self.bookmarks_text {
            self.bookmarks_text = text;
            self.rebuild();
        }
    }

    // NetworkMounts.rebuild: live mounts first, each labelled by its bookmark when one names it, then
    // every saved place nothing has mounted, one row per normalized URI.
    fn rebuild(&mut self) {
        let mut out: Vec<Share> = vec![];
        let mut seen = std::collections::HashSet::new();
        let marks: Vec<Bookmark> = mounts::bookmarks(&self.bookmarks_text);
        for m in mounts::parse_mounts(&self.listing_text) {
            let mut covered = false;
            for b in marks.iter().filter(|b| mounts::address_mount_covers(&m.uri, &b.uri)) {
                covered = true;
                if seen.insert(mounts::normalize(&b.uri)) {
                    out.push(Share { label: b.label.clone(), uri: b.uri.clone(), mount_uri: Some(m.uri.clone()), mounted: true });
                }
            }
            if covered || !seen.insert(mounts::normalize(&m.uri)) {
                continue;
            }
            let key = mounts::normalize(&m.uri);
            let label = marks.iter().find(|b| mounts::normalize(&b.uri) == key).map(|b| b.label.clone()).unwrap_or(m.label);
            out.push(Share { label, uri: m.uri, mount_uri: None, mounted: true });
        }
        for b in marks {
            if seen.insert(mounts::normalize(&b.uri)) {
                out.push(Share { label: b.label, uri: b.uri, mount_uri: None, mounted: false });
            }
        }
        self.entries = out;
    }

    pub fn unmount(&mut self, index: usize) -> Vec<Effect> {
        let Some(e) = self.entries.get(index).filter(|e| e.mounted) else { return vec![] };
        if self.unmount_label.is_some() {
            return vec![];
        }
        self.unmount_label = Some(e.label.clone());
        let target = e.mount_uri.clone().unwrap_or_else(|| e.uri.clone());
        vec![Effect::Unmount(argv(&["gio", "mount", "-u", &target]))]
    }

    pub fn unmounted(&mut self, ran: Ran) -> Vec<Effect> {
        let label = self.unmount_label.take().unwrap_or_default();
        let mut out = self.poll();
        out.push(match ran {
            Ran::Exited(Some(0), _) => Effect::Message(format!("Unmounted {label}."), false),
            _ => Effect::Message("That share could not be unmounted; it may still be in use.".into(), true),
        });
        out
    }
}

#[cfg(test)]
#[path = "network_tests.rs"]
mod tests;
