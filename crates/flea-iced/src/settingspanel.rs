// The settings panel's shell, ui/SettingsPanel.qml: "," opens it over the window, Tab moves between
// the section rail and the rows, Up and Down walk what the cursor can reach, Left and Right step a
// choice, Enter and Space act, Escape closes, and every other key stops here, because a panel that
// let one through would move the cursor in the listing behind it.
//
// A change is one patch through `flea --ui-state`, and the merged document that command prints is
// both what the panel draws next and what the window applies: the order, hidden files, the wrap,
// thumbnails, file type colours, the theme and the rail all change in this process. A key this port
// does not draw yet (density, columns, the preview column, text size, the menus) is still written,
// so the QML window and the next build read it.
use crate::places_rows::{self, nearest, following};
use crate::settings::{self, Facts, Kind, Row, SECTIONS};
use crate::{blocking, Flea, Message};
use iced::keyboard::{key::Named, Key};
use iced::Task;
use serde_json::{json, Value};

#[derive(Debug, Default)]
pub struct Panel {
    pub open: bool,
    // A section id: one of SECTIONS, or "columns", the View section's own sub-page.
    pub section: String,
    pub on_rail: bool,
    pub cursor: usize,
    pub doc: Value,
    pub facts: Option<Facts>,
    // The File types card, and the catalogue request it is waiting on (0 while none is out).
    pub card: Option<crate::openrulecard::Card>,
    pub catalogue_asked: u64,
    pub requests: u64,
    pub catalogue_error: String,
}

fn output(argv: &[&str]) -> String {
    std::process::Command::new(argv[0]).args(&argv[1..]).output().ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default()
}

// Omarchy's base size: the user's own shell.toml wins over the theme's, as Style.qml reads them.
pub(crate) fn base_size() -> u32 {
    let home = std::env::var("HOME").unwrap_or_default();
    for file in [".config/omarchy/shell.toml", ".local/state/omarchy/current/theme/shell.toml"] {
        let body = std::fs::read_to_string(format!("{home}/{file}")).unwrap_or_default();
        if let Some(n) = body.lines().find_map(|l| l.trim().strip_prefix("base-size").and_then(|r| r.trim().strip_prefix('=')).and_then(|v| v.trim().parse().ok())) {
            return n;
        }
    }
    12
}

// The facts the Display and About sections state, read once each time the panel opens.
fn gather(bin: &str) -> Facts {
    let monitors = output(&["hyprctl", "monitors", "-j"]);
    let scale = serde_json::from_str::<Value>(&monitors).ok().and_then(|v| v.as_array()?.iter().find(|m| m["focused"] == json!(true))?.get("scale")?.as_f64()).unwrap_or(0.0);
    Facts {
        home: std::env::var("HOME").unwrap_or_default(),
        base_size: base_size(),
        monitor_scale: scale,
        version: output(&[bin, "--version"]),
        handler: output(&["xdg-mime", "query", "default", "inode/directory"]),
        save_status: String::new(),
        apps: vec![],
    }
}

impl Flea {
    pub(crate) fn panel_message(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::PanelLoaded(doc, facts) => {
                self.panel_loaded(doc, *facts);
                Task::none()
            }
            Message::PanelSaved(doc) => self.panel_saved(doc),
            Message::PanelClose => {
                self.close_panel();
                Task::none()
            }
            Message::PanelSection(id) => {
                self.show_section(id);
                Task::none()
            }
            Message::PanelRemoveRule(i) => self.remove_rule(i),
            Message::Card(msg) => self.card_message(msg),
            Message::PanelActivate(i) => self.panel_activate(i),
            Message::PanelPick(i, value) => self.panel_pick(i, value),
            _ => Task::none(),
        }
    }

    pub(crate) fn panel_rows(&self) -> Vec<Row> {
        match &self.panel.facts {
            Some(f) => settings::rows(&self.panel.section, &self.panel.doc, f),
            None => vec![],
        }
    }

    pub(crate) fn open_panel(&mut self) -> Task<Message> {
        let bin = crate::backend::binary();
        self.panel.open = true;
        if self.panel.section.is_empty() {
            self.panel.section = "view".into();
        }
        self.panel.on_rail = false;
        Task::perform(blocking(move || {
            let doc = std::process::Command::new(&bin).arg("--ui-state").output().ok().and_then(|o| serde_json::from_slice(&o.stdout).ok());
            (doc, gather(&bin))
        }), |(doc, facts)| Message::PanelLoaded(doc, Box::new(facts)))
    }

    pub(crate) fn panel_loaded(&mut self, doc: Option<Value>, facts: Facts) {
        if let Some(d) = doc {
            self.panel.doc = d;
        }
        self.panel.facts = Some(facts);
        let rows = self.panel_rows();
        self.panel.cursor = settings::first_row(&rows);
    }

    // The catalogue goes with the panel, so an application installed meanwhile is listed next time.
    pub(crate) fn close_panel(&mut self) {
        self.panel.open = false;
        self.panel.card = None;
        self.panel.catalogue_asked = 0;
        if let Some(f) = self.panel.facts.as_mut() {
            f.apps.clear();
        }
    }

    pub(crate) fn show_section(&mut self, id: &str) {
        self.panel.section = id.to_string();
        // Every rule row draws its application's own Name=, so the catalogue is asked for here.
        if id == "open" {
            self.ask_catalogue();
        }
        let rows = self.panel_rows();
        self.panel.cursor = settings::first_row(&rows);
    }

    pub(crate) fn panel_write(&mut self, patch: Value) -> Task<Message> {
        let bin = crate::backend::binary();
        Task::perform(blocking(move || {
            let out = std::process::Command::new(&bin).arg("--ui-state").arg(patch.to_string()).output().ok()?;
            out.status.success().then(|| serde_json::from_slice(&out.stdout).ok()).flatten()
        }), Message::PanelSaved)
    }

    // A refused write keeps what is on screen and says so in the footer and the status bar.
    pub(crate) fn panel_saved(&mut self, doc: Option<Value>) -> Task<Message> {
        let Some(doc) = doc else {
            if let Some(f) = self.panel.facts.as_mut() {
                f.save_status = "Could not save that setting.".into();
            }
            return self.show("That setting could not be saved.".into(), true);
        };
        if let Some(f) = self.panel.facts.as_mut() {
            f.save_status = String::new();
        }
        let old = std::mem::replace(&mut self.panel.doc, doc);
        self.apply_settings(&old)
    }

    pub(crate) fn panel_activate(&mut self, index: usize) -> Task<Message> {
        let rows = self.panel_rows();
        let Some(r) = rows.get(index).cloned().filter(settings::focusable) else { return Task::none() };
        self.panel.cursor = index;
        let doc = self.panel.doc.clone();
        match r.kind {
            Kind::Favourite => {
                let fav = doc.pointer(&format!("/places/favourites/{}", r.id.trim_start_matches("favourite:"))).cloned();
                let path = fav.and_then(|f| f.get("path").and_then(Value::as_str).map(String::from)).unwrap_or_default();
                return if path.contains("://") && !path.starts_with("file://") { self.open_remote(&path, &r.label) } else {
                    let effects = self.pane.open(path, None);
                    self.run(effects)
                };
            }
            Kind::OpenRule => return self.open_card(&r.label, &r.caption),
            Kind::Group => {
                return match (r.action, r.id.as_str()) {
                    (Some("addFavourite"), _) => self.add_this_folder(),
                    (Some("addOpenRule"), _) => self.open_card("", ""),
                    (Some("pinFolder"), _) => self.say("The shelf is not in this build yet.".into()),
                    (_, "shelf.enabled") => self.panel_write(json!({ "shelf": { "enabled": r.state != "all" } })),
                    _ => self.panel_write(places_rows::menu_patch(&doc, &r.ids, true)),
                };
            }
            _ => {}
        }
        match r.id.as_str() {
            "columns" => {
                self.show_section("columns");
                Task::none()
            }
            "backView" => {
                self.show_section("view");
                Task::none()
            }
            "startFolder" => self.panel_write(json!({ "startFolder": self.pane.path })),
            "keyboardSheet" => self.say("The keyboard sheet is not in this build yet.".into()),
            "reportIssue" | "support" => {
                let url = if r.id == "support" { "https://github.com/sponsors/thisisgm" } else { "https://github.com/thisisgm/flea/issues" };
                let _ = std::process::Command::new("xdg-open").arg(url).spawn();
                Task::none()
            }
            id if id.starts_with("column:") => {
                let key = &id[7..];
                let mut shown: Vec<String> = doc.pointer("/columns").and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_else(|| vec!["name".into(), "size".into(), "date".into()]);
                match shown.iter().position(|c| c == key) {
                    Some(at) => {
                        shown.remove(at);
                    }
                    None => shown.push(key.to_string()),
                }
                self.panel_write(json!({ "columns": shown }))
            }
            "keyHints" => self.panel_write(json!({ "keyHints": !r.on })),
            _ if r.kind == Kind::Check && self.panel.section == "menus" => match menu_id(&r.id) {
                "" => Task::none(),
                id => self.panel_write(places_rows::menu_patch(&doc, &[id], false)),
            },
            _ if r.kind == Kind::Check => self.panel_write(settings::patch(&r.id, json!(!r.on))),
            _ => self.panel_step(index, 1),
        }
    }

    // stepRowValue: Follow and Override swap, the ruler steps a stop, a choice walks its values.
    pub(crate) fn panel_step(&mut self, index: usize, direction: isize) -> Task<Message> {
        let rows = self.panel_rows();
        let Some(r) = rows.get(index).cloned().filter(settings::focusable) else { return Task::none() };
        let base = self.panel.facts.as_ref().map(|f| f.base_size).unwrap_or(12);
        let doc = &self.panel.doc;
        let effective = if following(doc) { base } else { nearest(doc.pointer("/display/textSize/mode").and_then(Value::as_f64).unwrap_or(0.0) as u32) };
        match r.id.as_str() {
            "textMode" if following(doc) => self.panel_write(json!({ "display": { "textSize": { "mode": nearest(effective) } } })),
            "textMode" => self.panel_write(json!({ "display": { "textSize": { "mode": "system" } } })),
            "textStop" => {
                let at = places_rows::STOPS.iter().position(|s| *s == nearest(effective)).unwrap_or(0) as isize;
                let to = (at + direction).clamp(0, places_rows::STOPS.len() as isize - 1) as usize;
                self.panel_write(json!({ "display": { "textSize": { "mode": places_rows::STOPS[to] } } }))
            }
            _ if r.kind == Kind::Choice && !r.values.is_empty() => {
                let n = r.values.len() as isize;
                let target = ((r.selected as isize + direction) % n + n) % n;
                self.panel_write(settings::patch(&r.id, r.values[target as usize].clone()))
            }
            _ => Task::none(),
        }
    }

    // A choice's own segment, clicked: that value, not the next one.
    pub(crate) fn panel_pick(&mut self, index: usize, value: usize) -> Task<Message> {
        let rows = self.panel_rows();
        let Some(r) = rows.get(index).filter(|r| settings::focusable(r) && r.kind == Kind::Choice) else { return Task::none() };
        self.panel.cursor = index;
        if r.id == "textMode" {
            let follows = following(&self.panel.doc);
            return if (value == 0) != follows { self.panel_step(index, 1) } else { Task::none() };
        }
        match r.values.get(value).cloned() {
            Some(v) => self.panel_write(settings::patch(&r.id, v)),
            None => Task::none(),
        }
    }

    // "Add this folder": the folder the panel was opened over, as a favourite.
    fn add_this_folder(&mut self) -> Task<Message> {
        let path = self.pane.path.clone();
        let label = path.rsplit('/').next().filter(|l| !l.is_empty()).unwrap_or("/").to_string();
        let op = json!({ "op": "add", "record": { "label": label, "path": path } }).to_string();
        let bin = crate::backend::binary();
        Task::perform(blocking(move || {
            std::process::Command::new(&bin).arg("--favourites").arg(op).output().ok().filter(|o| o.status.success()).and_then(|o| serde_json::from_slice(&o.stdout).ok())
        }), Message::PanelSaved)
    }

    pub(crate) fn panel_key(&mut self, key: &Key) -> Task<Message> {
        if self.panel.card.is_some() {
            return self.card_key(key);
        }
        let rows = self.panel_rows();
        let on_rule = !self.panel.on_rail && rows.get(self.panel.cursor).is_some_and(|r| r.kind == Kind::OpenRule);
        match key.as_ref() {
            Key::Named(Named::Escape) => {
                self.close_panel();
                Task::none()
            }
            // SettingsRest rule 4: the row's own mark is x, so x on the cursor row is the same action.
            Key::Character("x") | Key::Named(Named::Delete) if on_rule => self.remove_rule(self.panel.cursor),
            Key::Named(Named::Tab) => {
                self.panel.on_rail = !self.panel.on_rail;
                Task::none()
            }
            Key::Named(Named::ArrowDown) | Key::Character("j") if self.panel.on_rail => self.rail_section(1),
            Key::Named(Named::ArrowUp) | Key::Character("k") if self.panel.on_rail => self.rail_section(-1),
            Key::Named(Named::ArrowDown) | Key::Character("j") => {
                self.panel.cursor = settings::step_row(&rows, self.panel.cursor, 1);
                Task::none()
            }
            Key::Named(Named::ArrowUp) | Key::Character("k") => {
                self.panel.cursor = settings::step_row(&rows, self.panel.cursor, -1);
                Task::none()
            }
            Key::Named(Named::ArrowRight) | Key::Character("l") if !self.panel.on_rail => self.panel_step(self.panel.cursor, 1),
            Key::Named(Named::ArrowLeft) | Key::Character("h") if !self.panel.on_rail => self.panel_step(self.panel.cursor, -1),
            Key::Named(Named::Enter | Named::Space) if self.panel.on_rail => {
                self.panel.on_rail = false;
                Task::none()
            }
            // Space acts only on a check or a heading; Enter acts on anything the cursor is on.
            Key::Named(Named::Space) if !rows.get(self.panel.cursor).is_some_and(|r| matches!(r.kind, Kind::Check | Kind::Group)) => Task::none(),
            Key::Named(Named::Enter | Named::Space) => self.panel_activate(self.panel.cursor),
            _ => Task::none(),
        }
    }

    fn rail_section(&mut self, delta: isize) -> Task<Message> {
        let at = SECTIONS.iter().position(|(id, _, _)| *id == self.panel.section).unwrap_or(0) as isize;
        let to = (at + delta).clamp(0, SECTIONS.len() as isize - 1) as usize;
        self.show_section(SECTIONS[to].0);
        Task::none()
    }
}

// A Menus row's id is the action id itself.
fn menu_id(id: &str) -> &'static str {
    const IDS: [&str; 24] = ["cut", "copy", "paste", "duplicate", "rename", "trash", "delete", "openwith", "openTerminal", "moveto", "copyto", "properties", "permissions", "copypath", "shelf", "compress", "extract", "convert", "taildrop", "localsend", "dropbox", "sharelink", "runScript", "placeMenu"];
    IDS.iter().copied().find(|i| *i == id).unwrap_or("")
}
