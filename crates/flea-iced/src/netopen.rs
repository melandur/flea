// Opening a network place: ui/NetworkMounts.qml openShare, the auth helper's exit codes, failMount
// and finishRequest. Up to three gio legs, each bounded at 15 s and only ever one at a time, and for
// a credentialed place one tools/flea-gio-auth run first. `gio info` decides every outcome; neither
// the mount's exit code nor the helper's is the verdict on its own (AGENTS.md "Issue #36", and "A
// credentialed connect that failed is asked to prove it").
use crate::mounts;
use crate::network::{argv, Effect, Leg, Network, Opening};
use crate::process::Ran;
use crate::sentence::{connect_failure, identity_question};

// The helper's own deadline: `timeout` answered 124 for it in QML, and a TimedOut here says the same.
pub const AUTH_TIMEOUT_MS: u64 = 35000;

pub fn helper() -> String {
    std::env::var("FLEA_GIO_AUTH").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| "/usr/lib/flea/flea-gio-auth".into())
}

impl Network {
    #[cfg(test)]
    pub fn busy(&self) -> bool {
        self.leg_running
    }

    pub fn open(&mut self, uri: &str, mounted: bool, label: &str) -> Vec<Effect> {
        self.open_with(uri, mounted, label, None)
    }

    // A place picked in the share browser carries the password its server was opened with.
    pub fn open_child(&mut self, uri: &str, label: &str) -> Vec<Effect> {
        let carried = self.opening_uri.as_ref().and_then(|u| self.passwords.get(u)).cloned();
        if let Some(p) = carried {
            self.passwords.insert(mounts::normalize(uri), p);
        }
        self.open(uri, false, label)
    }

    // NetworkMounts.saveLocation: the dialog's own attempt, answered by one Completed line.
    pub fn save_location(&mut self, uri: &str, label: &str, password: &str, id: &str) -> Vec<Effect> {
        self.open_with(uri, false, label, Some((id.to_string(), password.to_string())))
    }

    pub fn trust_once(&mut self, uri: &str) {
        self.trusted.insert(mounts::normalize(uri));
    }

    // The single-flight guard says so rather than returning in silence.
    fn open_with(&mut self, uri: &str, mounted: bool, label: &str, request: Option<(String, String)>) -> Vec<Effect> {
        if self.leg_running {
            let reason = "Another network location is still opening; give it a moment.".to_string();
            return match request {
                Some((id, _)) => vec![Effect::Completed { id, uri: mounts::normalize(uri), success: false, reason }],
                None => vec![Effect::Message(reason, false)],
            };
        }
        let normal = mounts::normalize(uri);
        self.generation += 1;
        self.opening_uri = Some(normal.clone());
        let password = request.as_ref().map(|(_, p)| p.clone()).filter(|p| !p.is_empty());
        self.opening = Some(Opening { uri: normal.clone(), label: label.to_string(), mount_failed: false, request, auth_failure: None });
        if mounted {
            return self.leg(Leg::Info, argv(&["gio", "info", &normal]));
        }
        if password.is_some() || mounts::credentialed(uri) {
            let password = password.or_else(|| self.passwords.get(&normal).cloned()).unwrap_or_default();
            if password.is_empty() {
                // With the keyring in use a plain mount reads it first; only a failure asks.
                if self.remember_passwords {
                    return self.leg(Leg::Mount, argv(&["gio", "mount", uri]));
                }
                return self.finish_or_ask(false, "Enter the password to mount this location.".into(), String::new(), false);
            }
            let store = if self.remember_passwords { "permanent" } else { "never" };
            let mut cmd = vec![helper(), normal.clone(), store.to_string()];
            // The trust is spent by this attempt; a second question means a second key to look at.
            if self.trusted.remove(&normal) {
                cmd.push("trust".into());
            }
            self.leg_running = true;
            return vec![Effect::Auth(self.generation, cmd, format!("{password}\n"))];
        }
        let mount = if uri.to_ascii_lowercase().starts_with("smb://") {
            argv(&["gio", "mount", "--anonymous", uri])
        } else {
            argv(&["gio", "mount", uri])
        };
        self.leg(Leg::Mount, mount)
    }

    fn leg(&mut self, which: Leg, argv: Vec<String>) -> Vec<Effect> {
        self.leg_running = true;
        vec![Effect::Leg(self.generation, which, argv)]
    }

    pub fn cancel(&mut self, id: &str) {
        if self.opening.as_ref().and_then(|o| o.request.as_ref()).is_some_and(|(r, _)| r == id) {
            // The running child is left to its deadline; its generation no longer answers.
            self.opening = None;
            self.leg_running = false;
            self.generation += 1;
        }
    }

    // finishRequest, or retryRequested when no dialog asked: either way the attempt says why.
    fn finish_or_ask(&mut self, success: bool, reason: String, attempted: String, failed: bool) -> Vec<Effect> {
        let Some(open) = self.opening.take() else { return vec![] };
        if let Some((id, password)) = open.request {
            if success && !password.is_empty() {
                self.passwords.insert(open.uri.clone(), password);
            }
            return vec![Effect::Completed { id, uri: open.uri, success, reason }];
        }
        if success {
            return vec![];
        }
        vec![Effect::AskPassword { uri: open.uri, label: open.label, password: attempted, reason, failed }]
    }

    // failMount: a refused password is forgotten, any other attempted one kept for the retry.
    fn fail(&mut self, reason: &str, refused: bool) -> Vec<Effect> {
        let Some(open) = self.opening.as_ref() else { return vec![] };
        let attempted = open.request.as_ref().map(|(_, p)| p.clone()).filter(|p| !p.is_empty())
            .or_else(|| self.passwords.get(&open.uri).cloned())
            .unwrap_or_default();
        let uri = open.uri.clone();
        if refused {
            self.passwords.remove(&uri);
        } else if !attempted.is_empty() {
            self.passwords.insert(uri, attempted.clone());
        }
        let mut out = vec![Effect::Message(reason.to_string(), true)];
        out.extend(self.finish_or_ask(false, reason.to_string(), attempted, true));
        out
    }

    // mountTimeout.onTriggered: the bar says so and a dialog's request is answered, but nothing
    // reopens the dialog, because a host that never answered was never asked for a password.
    fn timed_out(&mut self) -> Vec<Effect> {
        let reason = "Connect failed: host did not respond".to_string();
        let mut out = vec![Effect::Message(reason.clone(), true)];
        if let Some(open) = self.opening.take() {
            if let Some((id, _)) = open.request {
                out.push(Effect::Completed { id, uri: open.uri, success: false, reason });
            }
        }
        out
    }

    pub fn auth_done(&mut self, generation: u64, ran: Ran) -> Vec<Effect> {
        if generation != self.generation || self.opening.is_none() {
            return vec![];
        }
        self.leg_running = false;
        let (code, asked) = match ran {
            Ran::Exited(code, text) => (code, text),
            Ran::TimedOut => (Some(124), String::new()),
            Ran::NotStarted => (Some(127), String::new()),
        };
        let uri = self.opening.as_ref().unwrap().uri.clone();
        if code == Some(0) {
            return self.leg(Leg::Info, argv(&["gio", "info", &uri]));
        }
        // The identity question goes in front of a person, verbatim from gvfs, and is no failure.
        if code == Some(3) || code == Some(5) {
            return self.fail(&identity_question(&asked, code == Some(5), &uri), false);
        }
        let refused = !matches!(code, Some(124 | 126 | 127 | 3 | 4 | 5 | 6));
        let open = self.opening.as_mut().unwrap();
        open.auth_failure = Some((connect_failure(code, &uri), refused));
        open.mount_failed = true;
        // "Already mounted" is not a refused password: gio info is asked whether it opens anyway.
        self.leg(Leg::Info, argv(&["gio", "info", &uri]))
    }

    pub fn leg_done(&mut self, generation: u64, which: Leg, ran: Ran) -> Vec<Effect> {
        if generation != self.generation || self.opening.is_none() {
            return vec![];
        }
        self.leg_running = false;
        if ran == Ran::TimedOut {
            return self.timed_out();
        }
        let open = self.opening.as_mut().unwrap();
        match which {
            Leg::Mount => {
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
                    out.extend(self.finish_or_ask(true, String::new(), String::new(), false));
                    out.push(Effect::Opened(path));
                } else if mounts::is_bare_root(&open.uri) {
                    let uri = open.uri.clone();
                    out.extend(self.leg(Leg::List, argv(&["gio", "list", &uri])));
                } else if open.mount_failed {
                    let (reason, refused) = open.auth_failure.clone().unwrap_or(("Connect failed: network location was refused".into(), false));
                    out.extend(self.fail(&reason, refused));
                } else {
                    out.extend(self.fail("Connect failed: location has no browsable folder", false));
                }
                out
            }
            Leg::List => {
                let names: Vec<String> = match &ran {
                    Ran::Exited(Some(0), text) => text.lines().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect(),
                    _ => vec![],
                };
                if names.is_empty() {
                    return self.fail("Connect failed: location has no browsable folder", false);
                }
                let (base, label) = (open.uri.clone(), open.label.clone());
                let mut out = self.finish_or_ask(true, String::new(), String::new(), false);
                out.push(Effect::SharesListed { base, label, names });
                out
            }
        }
    }
}

#[cfg(test)]
#[path = "netopen_tests.rs"]
mod tests;
