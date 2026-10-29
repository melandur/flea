// The split view, ui/shell.qml's dual mode: two panes side by side, each with its own backend child,
// the primary on the left. The focused pane is the shell's own `pane`; the other one waits here in a
// Slot, and a message that belongs to it is run by swapping it in, handling the message exactly as
// the focused pane's would be, and swapping it back. That keeps one pane model and one effect runner
// for both, which is the whole point: neither pane has a second, lesser code path.
//
// Leaving the split drops the second pane and quits its backend rather than hiding it, which is
// AGENTS.md rule 6: a hidden view is not a free view. Re-entering lists the folder again.
use crate::backend::Backend;
use crate::pane::Pane;
use crate::wire::Request;
use crate::{blocking, sleep, Flea, Message, QUIT_DEADLINE};
use iced::Task;
use std::sync::Arc;

pub struct Slot {
    pub side: u8,
    pub pane: Pane,
    pub backend: Option<Arc<Backend>>,
    pub key: u64,
    pub scroll_y: f32,
}

impl Flea {
    // The width one pane draws in: the area beside the rail, halved while split.
    pub(crate) fn pane_width(&self) -> f32 {
        let area = self.window_w - if self.rail.shown { self.rail.width } else { 0.0 };
        if self.dual() { area / 2.0 } else { area }
    }

    // Both backends are asked and both must answer: each drains, so a copy in flight in either
    // removes its own partial destination before the window ends (shell.qml quitBackends).
    pub(crate) fn quit(&mut self) -> Task<Message> {
        if self.quitting {
            return Task::none();
        }
        self.quitting = true;
        let other = self.other.as_ref().and_then(|o| o.backend.clone().map(|b| (o.key, b)));
        let mine = self.backend.clone().map(|b| (self.backend_key, b));
        for (key, b) in mine.into_iter().chain(other) {
            if b.send(&Request::Quit).is_ok() {
                self.quit_keys.push(key);
            }
        }
        if self.quit_keys.is_empty() {
            return iced::exit();
        }
        Task::perform(sleep(QUIT_DEADLINE), |_| Message::QuitDeadline)
    }

    pub(crate) fn dual(&self) -> bool {
        self.other.is_some()
    }

    // Swap the waiting pane in for the focused one, everything a pane owns travelling with it.
    pub(crate) fn swap_other(&mut self) {
        let Some(o) = self.other.as_mut() else { return };
        std::mem::swap(&mut self.pane, &mut o.pane);
        std::mem::swap(&mut self.backend, &mut o.backend);
        std::mem::swap(&mut self.backend_key, &mut o.key);
        std::mem::swap(&mut self.scroll_y, &mut o.scroll_y);
        std::mem::swap(&mut self.side, &mut o.side);
    }

    // Run `f` as the pane on `side`: directly for the focused one, through a swap for the other, and
    // not at all for a side that is gone, which is a late message from a pane the split dropped.
    pub(crate) fn with_side(&mut self, side: u8, f: impl FnOnce(&mut Flea) -> Task<Message>) -> Task<Message> {
        if side == self.side {
            return f(self);
        }
        if self.other.as_ref().is_none_or(|o| o.side != side) {
            return Task::none();
        }
        self.swap_other();
        let task = f(self);
        self.swap_other();
        task
    }

    pub(crate) fn with_key(&mut self, key: u64, f: impl FnOnce(&mut Flea) -> Task<Message>) -> Task<Message> {
        if key == self.backend_key {
            return f(self);
        }
        match self.other.as_ref().filter(|o| o.key == key).map(|o| o.side) {
            Some(side) => self.with_side(side, f),
            None => Task::none(),
        }
    }

    pub(crate) fn pane_like(&self) -> Pane {
        let p = &self.pane;
        Pane::new(p.hidden, p.visible_rows).with_order(p.order.clone(), p.wrap_at_ends).with_view(&p.thumb_mode, p.file_type_colours)
    }

    pub(crate) fn spawn_key(&mut self) -> u64 {
        self.spawns += 1;
        self.spawns
    }

    // Ctrl+T. Entering duplicates the folder the split was entered from (the 2026-10-08 ruling) and
    // hands the keyboard to the pane that just appeared (2026-09-18); leaving puts it back on the
    // primary, the only pane left.
    pub(crate) fn toggle_split(&mut self) -> Task<Message> {
        self.rail.focused = false;
        if self.dual() {
            if self.side != 0 {
                self.swap_other();
            }
            if let Some(o) = self.other.take() {
                if let Some(b) = o.backend {
                    let _ = b.send(&crate::wire::Request::Quit);
                }
            }
            return self.save_view(false);
        }
        let opened = self.open_second(self.pane.path.clone(), true);
        Task::batch([opened, self.save_view(true)])
    }

    pub(crate) fn open_second(&mut self, path: String, focus: bool) -> Task<Message> {
        let key = self.spawn_key();
        let backend = match Backend::spawn(key) {
            Ok(b) => b,
            Err(e) => return self.say(e),
        };
        let pane = self.pane_like();
        self.other = Some(Slot { side: 1, pane, backend: Some(backend), key, scroll_y: 0.0 });
        let mut task = self.with_side(1, |f| {
            let effects = f.pane.open(path, None);
            f.run(effects)
        });
        if focus {
            self.swap_other();
            task = Task::batch([task, self.save_focus()]);
        }
        task
    }

    // Tab in the listing while split: the keyboard goes to the other pane.
    pub(crate) fn switch_pane(&mut self) -> Task<Message> {
        if !self.dual() {
            return Task::none();
        }
        self.swap_other();
        self.save_focus()
    }

    // A click in the unfocused pane focuses it before the click lands.
    pub(crate) fn focus_side(&mut self, side: u8) -> Task<Message> {
        if side == self.side || !self.dual() {
            return Task::none();
        }
        self.switch_pane()
    }

    // ViewState.toggleDual: the view becomes "dual" with the one it came from remembered, and goes
    // back to that one on leaving; both are one-leaf patches through `flea --ui-state`.
    fn save_view(&mut self, dual: bool) -> Task<Message> {
        let patch = if dual {
            serde_json::json!({ "view": "dual", "dual": { "from": self.dual_from } })
        } else {
            serde_json::json!({ "view": self.dual_from })
        };
        save(patch)
    }

    // rememberDual: which pane holds the keyboard, so a window left split restores it.
    fn save_focus(&mut self) -> Task<Message> {
        save(serde_json::json!({ "dual": { "focus": self.side } }))
    }
}

// The view key alone, for a mode chosen outside the split.
pub(crate) fn save_view_mode(mode: &str) -> Task<Message> {
    save(serde_json::json!({ "view": mode }))
}

fn save(patch: serde_json::Value) -> Task<Message> {
    let bin = crate::backend::binary();
    Task::perform(
        blocking(move || {
            std::process::Command::new(&bin).arg("--ui-state").arg(patch.to_string()).output().map(|o| o.status.success()).unwrap_or(false)
        }),
        Message::Saved,
    )
}

impl Flea {
    // A backend's line, routed to the pane whose spawn it came from. Only the focused pane drives the
    // chrome's Tab completion and the preview, so those two answers are read for it alone.
    pub(crate) fn backend_event(&mut self, key: u64, event: crate::backend::Event) -> Task<Message> {
        use crate::backend::Event;
        use crate::wire::Reply;
        match event {
            Event::Reply(reply @ Reply::Peeked { .. }) if key == self.backend_key => self.peeked(reply),
            Event::Reply(Reply::Installed { id, apps }) if key == self.backend_key => {
                self.catalogue_answered(id, apps);
                Task::none()
            }
            Event::Reply(Reply::MenuAction(v)) if key == self.backend_key => self.menuaction_reply(&v),
            Event::Reply(Reply::Job(v)) => self.with_key(key, |f| f.job_reply(&v)),
            Event::Reply(Reply::Permissions(v)) if key == self.backend_key => self.action_reply(&v, true),
            Event::Reply(reply @ Reply::Meta { .. }) if key == self.backend_key => {
                self.meta_answered(reply);
                Task::none()
            }
            Event::Reply(reply) => self.with_key(key, |f| {
                let effects = f.pane.receive(reply);
                f.run(effects)
            }),
            Event::Unreadable => self.say("The backend sent a line this build cannot read.".into()),
            Event::Exited(code) => {
                if self.quitting {
                    self.quit_keys.retain(|k| *k != key);
                    return if self.quit_keys.is_empty() { iced::exit() } else { Task::none() };
                }
                // A second pane's child quitting because the split was left says nothing.
                let mine = key == self.backend_key;
                let theirs = self.other.as_ref().is_some_and(|o| o.key == key);
                if !mine && !theirs {
                    return Task::none();
                }
                if mine {
                    self.backend = None;
                } else if let Some(o) = self.other.as_mut() {
                    o.backend = None;
                }
                let code = code.map(|c| c.to_string()).unwrap_or_else(|| "a signal".into());
                self.say(format!("The backend exited with code {code}."))
            }
        }
    }
}
