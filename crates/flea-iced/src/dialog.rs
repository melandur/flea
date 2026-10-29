// The connect dialog's flow, ui/NetworkDialog.qml: open blank or on a place, submit one attempt,
// read its one Completed answer, then save it as a favourite through `flea --favourites`. A host key
// question turns the button into "Connect anyway", and pressing it trusts that one attempt.
use crate::form::{values_for, Form};
use crate::mounts::normalize;
use crate::network;
use crate::sentence::is_question;
use crate::{blocking, Flea, Message};
use iced::Task;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Field {
    Label,
    Host,
    Port,
    Path,
    Domain,
    User,
    Password,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Open,
    Close,
    Pick(&'static str),
    Tls(bool),
    Field(Field, String),
    Submit,
    Eye,
}

#[derive(Debug, Default)]
pub struct Dialog {
    pub form: Form,
    pub status: String,
    pub retrying: bool,
    pub failed_connect: bool,
    pub verifying: bool,
    pub connecting: bool,
    pub saving: bool,
    pub show_password: bool,
    request_serial: u64,
    request_id: String,
    pending_uri: String,
    pending_label: String,
    mounted_uri: String,
    save_new_place: bool,
    // Edit: the saved place this attempt rewrites once it connects (RailMenu.editPlace).
    pub(crate) editing: Option<String>,
}

impl Dialog {
    pub fn busy(&self) -> bool {
        self.connecting || self.saving
    }

    pub fn title(&self) -> String {
        let base = if self.retrying { "Connect to a network share" } else { "Add a network share" };
        if self.failed_connect { format!("{base}, failed connect") } else { base.to_string() }
    }

    pub fn action(&self) -> &'static str {
        if self.saving {
            "Saving..."
        } else if self.connecting {
            "Connecting..."
        } else if self.verifying {
            "Connect anyway"
        } else if self.retrying {
            "Retry"
        } else {
            "Connect and save"
        }
    }

    pub fn field_mut(&mut self, f: Field) -> &mut String {
        match f {
            Field::Label => &mut self.form.label,
            Field::Host => &mut self.form.host,
            Field::Port => &mut self.form.port,
            Field::Path => &mut self.form.path,
            Field::Domain => &mut self.form.domain,
            Field::User => &mut self.form.user,
            Field::Password => &mut self.form.password,
        }
    }
}

impl Flea {
    pub(crate) fn dialog_message(&mut self, msg: Msg) -> Task<Message> {
        match msg {
            Msg::Open => return self.dialog_open(),
            Msg::Close => return self.dialog_close(),
            Msg::Submit => return self.dialog_submit(),
            _ => {}
        }
        let Some(d) = self.dialog.as_mut() else { return Task::none() };
        match msg {
            Msg::Pick(name) => d.form.pick(name),
            Msg::Tls(on) => d.form.set_tls(on),
            Msg::Field(which, value) => *d.field_mut(which) = value,
            Msg::Eye => d.show_password = !d.show_password,
            Msg::Open | Msg::Close | Msg::Submit => {}
        }
        Task::none()
    }

    pub(crate) fn dialog_open(&mut self) -> Task<Message> {
        if self.dialog.as_ref().is_some_and(Dialog::busy) {
            return Task::none();
        }
        self.dialog = Some(Dialog { save_new_place: true, ..Default::default() });
        iced::widget::operation::focus(crate::dialogview::HOST_ID)
    }

    // retryRequested and Edit: the form reopened on the place, the reason in its status line.
    pub(crate) fn dialog_open_on(&mut self, uri: &str, label: &str, password: &str, reason: &str, failed: bool) -> Task<Message> {
        if self.dialog.as_ref().is_some_and(Dialog::busy) {
            return Task::none();
        }
        // A connect failure kept across a close is what this place says, once, instead of the
        // generic password prompt; showing it is what acknowledges it.
        let known = self.dialog_memory.clone().filter(|(u, _)| *u == normalize(uri));
        let (status, failed_connect) = match (&known, failed) {
            (Some((_, r)), false) => (r.clone(), true),
            _ => (reason.to_string(), failed),
        };
        if known.is_some() {
            self.dialog_memory = None;
        }
        self.dialog = Some(Dialog { form: values_for(uri, label, password), status, retrying: true, failed_connect, ..Default::default() });
        iced::widget::operation::focus(crate::dialogview::HOST_ID)
    }

    pub(crate) fn dialog_close(&mut self) -> Task<Message> {
        let Some(d) = self.dialog.as_mut() else { return Task::none() };
        if d.saving {
            return Task::none();
        }
        if d.connecting {
            let id = d.request_id.clone();
            self.network.cancel(&id);
        }
        // The password goes with the dialog; a failure it showed stays in dialog_memory.
        self.dialog = None;
        Task::none()
    }

    pub(crate) fn dialog_submit(&mut self) -> Task<Message> {
        let Some(d) = self.dialog.as_mut() else { return Task::none() };
        if d.busy() {
            return Task::none();
        }
        if !d.form.complete() {
            d.status = "Enter a valid host and port.".into();
            return Task::none();
        }
        if d.form.spec().credentials && !d.form.user.trim().is_empty() && d.form.password.is_empty() {
            d.status = "Enter the password to mount this location.".into();
            d.retrying = true;
            d.failed_connect = false;
            return Task::none();
        }
        let uri = normalize(&d.form.uri());
        // The answer to the identity question rides this submit: trust first, then the attempt.
        if d.verifying {
            d.verifying = false;
            self.network.trust_once(&uri);
        }
        let d = self.dialog.as_mut().unwrap();
        d.pending_uri = uri.clone();
        d.pending_label = d.form.label_text();
        d.status.clear();
        self.dialog_memory = None;
        let d = self.dialog.as_mut().unwrap();
        d.request_serial += 1;
        d.request_id = format!("network-{}", d.request_serial);
        if d.mounted_uri == d.pending_uri {
            return self.dialog_save();
        }
        d.connecting = true;
        let (label, password, id) = (d.pending_label.clone(), d.form.password.clone(), d.request_id.clone());
        let effects = self.network.save_location(&uri, &label, &password, &id);
        self.run_network(effects)
    }

    pub(crate) fn dialog_finished(&mut self, id: &str, uri: &str, success: bool, reason: &str) -> Task<Message> {
        let Some(d) = self.dialog.as_mut() else { return Task::none() };
        if !d.connecting || id != d.request_id || uri != d.pending_uri {
            return Task::none();
        }
        d.connecting = false;
        if !success {
            return self.dialog_failed(reason.to_string());
        }
        d.mounted_uri = uri.to_string();
        self.dialog_memory = None;
        let d = self.dialog.as_mut().unwrap();
        // RailMenu.placeSaved: an edited address that connected replaces the saved line.
        if let Some(was) = d.editing.take().filter(|was| *was != normalize(uri)) {
            let (new, label) = (d.pending_uri.clone(), d.pending_label.clone());
            self.replace_share(&was, &new, &label);
        }
        self.dialog_save()
    }

    fn dialog_save(&mut self) -> Task<Message> {
        let Some(d) = self.dialog.as_mut() else { return Task::none() };
        if !d.save_new_place {
            self.dialog = None;
            return Task::none();
        }
        d.saving = true;
        let op = serde_json::json!({ "op": "add", "record": { "label": d.pending_label, "path": d.pending_uri } }).to_string();
        let bin = crate::backend::binary();
        Task::perform(blocking(move || add_favourite(&bin, &op)), Message::FavouriteSaved)
    }

    pub(crate) fn favourite_saved(&mut self, result: Result<Vec<Value>, String>) -> Task<Message> {
        let Some(d) = self.dialog.as_mut() else { return Task::none() };
        d.saving = false;
        match result {
            Err(message) => self.dialog_failed(message),
            Ok(records) => {
                self.rail.favourites = records;
                self.rebuild_rail();
                self.dialog = None;
                Task::none()
            }
        }
    }

    fn dialog_failed(&mut self, message: String) -> Task<Message> {
        let Some(d) = self.dialog.as_mut() else { return Task::none() };
        d.saving = false;
        d.verifying = is_question(&message);
        d.retrying = true;
        d.failed_connect = !d.verifying && message.starts_with("Connect failed:");
        if d.failed_connect {
            self.dialog_memory = Some((d.pending_uri.clone(), message.clone()));
        }
        let d = self.dialog.as_mut().unwrap();
        d.status = message;
        Task::none()
    }

    // The network service's answers that belong to the dialog.
    pub(crate) fn dialog_effect(&mut self, e: network::Effect) -> Task<Message> {
        match e {
            network::Effect::Completed { id, uri, success, reason } => self.dialog_finished(&id, &uri, success, &reason),
            network::Effect::AskPassword { uri, label, password, reason, failed } => self.dialog_open_on(&uri, &label, &password, &reason, failed),
            _ => Task::none(),
        }
    }
}

// `flea --favourites` prints the whole merged document on success and one "flea: " line on failure.
pub(crate) fn add_favourite(bin: &str, op: &str) -> Result<Vec<Value>, String> {
    let out = std::process::Command::new(bin).arg("--favourites").arg(op).output().map_err(|_| "Favorites updater could not start.".to_string())?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let line = err.lines().next().unwrap_or("").trim_start_matches("flea: ").to_string();
        return Err(if line.is_empty() { "Favorites could not be saved.".into() } else { line });
    }
    let state: Value = serde_json::from_slice(&out.stdout).map_err(|_| "Favorites could not be saved.".to_string())?;
    state.pointer("/places/favourites").and_then(Value::as_array).cloned().ok_or_else(|| "Favorites could not be saved.".into())
}
