// The menu rows that start a job: Compress, Extract, Convert and Run script, through activation.rs's
// snapshot and activate, and Open with, through its own card. This owns the menu's probe facts (the
// backend's `formats` answer), the archive and convert lines while a job runs, and the sentences the
// status bar says when one ends: ui/PaneWire.qml's onArchive* and onConvert*, and ui/js/Ops.js.
use crate::activation::{Activation, Step};
use crate::convertcard::Convert;
use crate::openwith::{self, OpenWith};
use crate::pane::{join, sentence, Effect};
use crate::wire::Request;
use crate::{archives, blocking, scripts, Flea, Message};
use iced::Task;
use serde_json::{json, Value};

// The job rows' state on the window: the activation being checked, the open card, and the probe.
#[derive(Debug, Default)]
pub struct Jobs {
    pub activation: Option<Activation>,
    pub convert: Option<Convert>,
    pub open_with: Option<OpenWith>,
    pub formats: Formats,
}

#[derive(Debug, Clone, Default)]
pub struct Formats {
    pub archive: Vec<String>,
    pub convert: bool,
    pub extract: bool,
}

fn leaf(path: &str) -> String {
    path.trim_end_matches('/').rsplit('/').next().unwrap_or("").to_string()
}

impl Flea {
    fn send_json(&mut self, v: Value) -> Task<Message> {
        self.run(vec![Effect::Send(Request::Json(v))])
    }

    // The probe the menu reads, asked at startup and again on every menu so a tool installed while
    // the window was open is offered from the next menu on.
    pub(crate) fn ask_formats(&mut self) -> Task<Message> {
        self.send_json(json!({"c": "formats", "id": 1}))
    }

    pub(crate) fn menu_targets(&self) -> Vec<usize> {
        if self.pane.selection.is_empty() { vec![self.pane.cursor] } else { self.pane.selection.iter().copied().collect() }
    }

    // A row that acts at once: compress:<format>, extract, convert, runScript:<id> or a send row.
    pub(crate) fn activate_row(&mut self, action: String) -> Task<Message> {
        self.activate_to(action, None)
    }

    pub(crate) fn activate_to(&mut self, action: String, dest: Option<String>) -> Task<Message> {
        if self.jobs.activation.is_some() {
            return self.say("The last menu action is still being checked.".into());
        }
        self.card_ids += 1;
        let (a, snapshot) = Activation::to(self.card_ids, &action, &self.menu_targets(), self.pane.cursor, dest);
        self.jobs.activation = Some(a);
        self.send_json(snapshot)
    }

    pub(crate) fn open_with_row(&mut self) -> Task<Message> {
        if self.jobs.open_with.is_some() || self.card.is_some() {
            return self.say("The deletion is still finishing.".into());
        }
        self.card_ids += 1;
        let (card, snapshot) = OpenWith::open(self.card_ids, &self.menu_targets(), self.pane.cursor);
        self.jobs.open_with = Some(card);
        self.send_json(snapshot)
    }

    // Every menuaction answer: whichever of the three exchanges owns its id takes it.
    pub(crate) fn menuaction_reply(&mut self, v: &Value) -> Task<Message> {
        let id = v.get("id").and_then(Value::as_u64);
        if let Some(a) = self.jobs.activation.as_mut().filter(|a| Some(a.id) == id) {
            return match a.receive(v) {
                Step::Nothing => Task::none(),
                Step::Send(line) => self.send_json(line),
                Step::Refused(line) => {
                    self.jobs.activation = None;
                    self.show(line, true)
                }
                Step::Done { action, paths } => {
                    let menu_id = a.id;
                    self.jobs.activation = None;
                    self.activated(&action, menu_id, paths)
                }
            };
        }
        if let Some(card) = self.jobs.open_with.as_mut().filter(|c| Some(c.id) == id) {
            let out = card.receive(v);
            return self.carry_open_with(out);
        }
        self.action_reply(v, false)
    }

    pub(crate) fn carry_open_with(&mut self, out: Vec<openwith::Out>) -> Task<Message> {
        let mut tasks = vec![];
        for o in out {
            match o {
                openwith::Out::Send(v) => tasks.push(self.send_json(v)),
                openwith::Out::Close => self.jobs.open_with = None,
            }
        }
        Task::batch(tasks)
    }

    fn activated(&mut self, action: &str, menu_id: u64, paths: Vec<String>) -> Task<Message> {
        if action == "addToShelf" || action == "dropbox" || action == "sharelink" || action.starts_with("taildrop:") || action.starts_with("localsend:") {
            return self.send_activated(action, menu_id, paths);
        }
        let row = self.pane.row_at(self.pane.cursor).cloned();
        if let Some(format) = action.strip_prefix("compress:") {
            // Ops.compressResolved: one row under its own name, several under the folder's.
            let names: Vec<String> = paths.iter().map(|p| leaf(p)).collect();
            if names.is_empty() {
                return Task::none();
            }
            let stem = archives::archive_stem(&names, &leaf(&self.pane.path));
            let dest = join(&self.pane.path, &format!("{stem}.{format}"));
            let work = format!("Compressing {} to .{format}", if names.len() == 1 { "1 item".to_string() } else { format!("{} items", names.len()) });
            self.pane.ops.archive = Some((work.clone(), false));
            self.pane.sticky = work;
            return self.send_json(json!({"c": "archive", "op": "compress", "paths": paths, "dest": dest, "format": format, "menuId": menu_id}));
        }
        if let Some(id) = action.strip_prefix("runScript:") {
            let id = id.to_string();
            return Task::perform(blocking(move || scripts::run(&id, &paths)), |r| Message::Job(crate::jobkeys::Msg::Script(r.err())));
        }
        let Some(row) = row else { return Task::none() };
        let path = join(&self.pane.path, &row.n);
        match action {
            // Extract unpacks beside the archive, into a directory named after it.
            "extract" => {
                let work = format!("Extracting {}", row.n);
                self.pane.ops.archive = Some((work.clone(), true));
                self.pane.sticky = work;
                let dest = join(&self.pane.path, &archives::extract_dir(&row.n));
                self.send_json(json!({"c": "archive", "op": "extract", "path": path, "dest": dest, "menuId": menu_id}))
            }
            "convert" if !row.d => {
                let (card, out) = Convert::open(&path, &row.n, menu_id);
                self.jobs.convert = Some(card);
                Task::batch(out.into_iter().map(|v| self.send_json(v)).collect::<Vec<_>>())
            }
            _ => Task::none(),
        }
    }

    // archive*, convert* and formats, from the pane whose backend wrote them.
    pub(crate) fn job_reply(&mut self, v: &Value) -> Task<Message> {
        let t = v.get("t").and_then(Value::as_str).unwrap_or("");
        let n = |k: &str| v.get(k).and_then(Value::as_u64).unwrap_or(0);
        let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let ok = v.get("ok") == Some(&json!(true));
        match t {
            "localsendpeers" | "localsendsent" => self.localsend_answered(v),
            "formats" => {
                let providers = self.providers_answered(v);
                self.jobs.formats = Formats {
                    archive: v.get("archive").and_then(Value::as_array).into_iter().flatten().filter_map(|f| f.as_str().map(String::from)).collect(),
                    convert: v.get("convert") == Some(&json!(true)),
                    extract: v.pointer("/extract/archive") == Some(&json!(true)),
                };
                providers
            }
            "archiveprogress" => {
                if let Some((work, _)) = &self.pane.ops.archive {
                    self.pane.sticky = archives::progress_line(work, n("done"), n("total"));
                }
                Task::none()
            }
            "archivedone" => {
                let extracting = self.pane.ops.archive.take().is_some_and(|(_, e)| e);
                self.pane.sticky.clear();
                let line = if ok { archives::done_line(v.get("verified") == Some(&json!(true)), extracting).to_string() } else { sentence("archive", &s("err"), "") };
                let said = self.show(line, !ok);
                let effects = self.pane.refresh_here();
                Task::batch([said, self.run(effects)])
            }
            "convertchecked" => {
                let out = self.jobs.convert.as_mut().map(|c| c.checked(v)).unwrap_or_default();
                Task::batch(out.into_iter().map(|line| self.send_json(line)).collect::<Vec<_>>())
            }
            "convertstarted" => {
                if let Some(c) = self.jobs.convert.as_mut() {
                    c.started(v);
                }
                Task::none()
            }
            "convertdone" => {
                match self.jobs.convert.as_mut().and_then(|c| c.done(v)) {
                    None => return Task::none(),
                    Some(true) => self.jobs.convert = None,
                    Some(false) => {}
                }
                self.pane.sticky.clear();
                let collision = v.get("collision") == Some(&json!(true));
                let line = if ok { format!("Converted to {}.", leaf(&s("path"))) } else if collision { s("err") } else { sentence("convert", &s("err"), "") };
                let said = self.show(line, !ok);
                if !ok {
                    return said;
                }
                let effects = self.pane.refresh_on(leaf(&s("path")), self.pane.cursor, false);
                Task::batch([said, self.run(effects)])
            }
            _ => Task::none(),
        }
    }
}
