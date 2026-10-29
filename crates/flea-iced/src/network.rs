// The NETWORK group's service, ui/NetworkMounts.qml and ui/MountListing.qml with no toolkit in them.
//
// Opening a place is up to three legs, each bounded at 15 s and only ever one at a time: `gio mount`,
// then `gio info` for the FUSE path, then for a bare server root `gio list` for its shares. The mount
// leg's exit code is never the verdict: gvfsd's "already mounted" refusal is translated and exits 2
// on a share that is perfectly openable, so `gio info` decides every time (AGENTS.md "Issue #36").
// Signing in is Stage 4b-ii; a place that needs a password says so rather than failing silently.
use crate::mounts::{self, Bookmark};
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
    Leg(Leg, Vec<String>),
    Unmount(Vec<String>),
    Message(String, bool),
    Opened(String),
    SharesListed { base: String, label: String, names: Vec<String> },
}

#[derive(Debug)]
struct Opening {
    uri: String,
    label: String,
    mount_failed: bool,
}

#[derive(Debug, Default)]
pub struct Network {
    pub entries: Vec<Share>,
    pub remember_passwords: bool,
    listing_text: String,
    bookmarks_text: String,
    listing: bool,
    poll_again: bool,
    opening: Option<Opening>,
    leg_running: bool,
    unmount_label: Option<String>,
}

fn argv(parts: &[&str]) -> Vec<String> {
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

    #[cfg(test)]
    pub fn busy(&self) -> bool {
        self.leg_running
    }

    // The single-flight guard says so rather than returning in silence.
    pub fn open(&mut self, uri: &str, mounted: bool, label: &str) -> Vec<Effect> {
        if self.leg_running {
            return vec![Effect::Message("Another network location is still opening; give it a moment.".into(), false)];
        }
        let normal = mounts::normalize(uri);
        self.opening = Some(Opening { uri: normal.clone(), label: label.to_string(), mount_failed: false });
        if mounted {
            return self.leg(Leg::Info, argv(&["gio", "info", &normal]));
        }
        if mounts::credentialed(uri) && !self.remember_passwords {
            // NetworkMounts asks for the password here; that dialog is Stage 4b-ii.
            self.opening = None;
            return vec![Effect::Message("Signing in to a network location is not in this build yet.".into(), false)];
        }
        // With remembered passwords on, a plain mount is what reads the login keyring.
        let mount = if uri.to_ascii_lowercase().starts_with("smb://") && !mounts::credentialed(uri) {
            argv(&["gio", "mount", "--anonymous", uri])
        } else {
            argv(&["gio", "mount", uri])
        };
        self.leg(Leg::Mount, mount)
    }

    fn leg(&mut self, which: Leg, argv: Vec<String>) -> Vec<Effect> {
        self.leg_running = true;
        vec![Effect::Leg(which, argv)]
    }

    fn fail(&mut self, reason: &str) -> Vec<Effect> {
        self.opening = None;
        vec![Effect::Message(reason.to_string(), true)]
    }

    pub fn leg_done(&mut self, which: Leg, ran: Ran) -> Vec<Effect> {
        self.leg_running = false;
        let Some(open) = self.opening.as_mut() else { return vec![] };
        if ran == Ran::TimedOut {
            return self.fail("Connect failed: host did not respond");
        }
        match which {
            Leg::Mount => {
                // The code is recorded and never decides: gio info is asked whatever it was.
                open.mount_failed = !matches!(ran, Ran::Exited(Some(0), _));
                let uri = open.uri.clone();
                self.leg(Leg::Info, argv(&["gio", "info", &uri]))
            }
            Leg::Info => {
                let mut out = self.poll();
                let path = match &ran {
                    Ran::Exited(Some(0), text) => mounts::local_path(text),
                    _ => String::new(),
                };
                let open = self.opening.as_ref().unwrap();
                if !path.is_empty() {
                    self.opening = None;
                    out.push(Effect::Opened(path));
                } else if mounts::is_bare_root(&open.uri) {
                    // No exit code gate here either: some reachable roots refuse `gio info`.
                    let uri = open.uri.clone();
                    out.extend(self.leg(Leg::List, argv(&["gio", "list", &uri])));
                } else if open.mount_failed {
                    out.extend(self.fail("Connect failed: network location was refused"));
                } else {
                    out.extend(self.fail("Connect failed: location has no browsable folder"));
                }
                out
            }
            Leg::List => {
                let names: Vec<String> = match &ran {
                    Ran::Exited(Some(0), text) => text.lines().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect(),
                    _ => vec![],
                };
                if names.is_empty() {
                    return self.fail("Connect failed: location has no browsable folder");
                }
                let open = self.opening.take().unwrap();
                vec![Effect::SharesListed { base: open.uri, label: open.label, names }]
            }
        }
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
