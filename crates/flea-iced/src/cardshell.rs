// The menu's dialog rows in the window: actioncard.rs decides, this carries its Outs to the backend,
// the status bar and the listing, owns the card's keyboard, and opens one from a menu row or Ctrl+M.
use crate::actioncard::{Card, Kind, Out, Phase, CHECK_MS};
use crate::wire::Request;
use crate::{sleep, Flea, Message};
use iced::keyboard::{key::Named, Key};
use iced::widget::operation;
use iced::Task;
use std::time::Duration;

pub const ACTION_FIELD: &str = "action-field";

#[derive(Debug, Clone)]
pub enum Msg {
    Input(String),
    Submit,
    Close,
    Confirm,
    Bit(u32),
    Check(u64),
}

impl Flea {
    pub(crate) fn action_message(&mut self, msg: Msg) -> Task<Message> {
        let Some(card) = self.card.as_mut() else { return Task::none() };
        let out = match msg {
            Msg::Input(text) => {
                card.input(text);
                vec![]
            }
            Msg::Submit => card.submit(),
            Msg::Close => card.close(),
            Msg::Confirm => card.confirm(),
            Msg::Bit(bit) => {
                card.toggle_bit(bit);
                vec![]
            }
            Msg::Check(generation) => card.check_due(generation),
        };
        self.carry(out)
    }

    pub(crate) fn action_reply(&mut self, v: &serde_json::Value, permissions: bool) -> Task<Message> {
        let Some(card) = self.card.as_mut() else { return Task::none() };
        let out = if permissions { card.receive_permissions(v) } else { card.receive_menu(v) };
        self.carry(out)
    }

    fn carry(&mut self, out: Vec<Out>) -> Task<Message> {
        let mut tasks = vec![];
        for o in out {
            match o {
                Out::Send(v) => tasks.push(self.run(vec![crate::pane::Effect::Send(Request::Json(v))])),
                Out::Close => self.card = None,
                Out::Say(line) => tasks.push(self.say(line)),
                Out::Alert(line) => tasks.push(self.show(line, true)),
                Out::Refresh(name) => {
                    let effects = if name.is_empty() { self.pane.refresh_here() } else { self.pane.refresh_on(name, self.pane.cursor, false) };
                    tasks.push(self.run(effects));
                }
                Out::Check(generation) => tasks.push(Task::perform(sleep(Duration::from_millis(CHECK_MS)), move |_| Message::Action(Msg::Check(generation)))),
            }
        }
        Task::batch(tasks)
    }

    // PaneMenuActions.open: one card at a time, and a deletion still running holds the slot.
    pub(crate) fn open_action(&mut self, kind: Kind) -> Task<Message> {
        if self.card.is_some() {
            return self.say("The deletion is still finishing.".into());
        }
        let rows: Vec<usize> = if self.pane.selection.is_empty() { vec![self.pane.cursor] } else { self.pane.selection.iter().copied().collect() };
        // Permissions takes the one target, which is the selected row when one is marked.
        let target = if rows.len() == 1 { rows[0] } else { self.pane.cursor };
        let path = self.pane.row_at(target).map(|r| crate::pane::join(&self.pane.path, &r.n)).unwrap_or_default();
        if kind != Kind::NewFile && path.is_empty() {
            return Task::none();
        }
        self.card_ids += 1;
        let (card, out) = Card::open(kind, self.card_ids, &self.pane.path, &rows, self.pane.cursor, &path);
        let takes_text = card.takes_text();
        self.card = Some(card);
        let sent = self.carry(out);
        if takes_text { Task::batch([sent, operation::focus(ACTION_FIELD), operation::select_all(ACTION_FIELD)]) } else { sent }
    }

    // The card owns the keyboard while it is drawn: Escape closes, Tab moves the confirmation's
    // focus off Cancel, Enter acts on what is focused; a field's own Enter arrives through on_submit.
    pub(crate) fn action_key(&mut self, key: &Key) -> Option<Task<Message>> {
        let card = self.card.as_mut().filter(|c| c.visible())?;
        let out = match key.as_ref() {
            Key::Named(Named::Escape) => card.close(),
            Key::Named(Named::Tab) if card.phase == Phase::Confirm => {
                card.focus = 1 - card.focus.min(1);
                vec![]
            }
            Key::Named(Named::Enter | Named::Space) if card.phase == Phase::Confirm => {
                if card.focus == 1 { card.confirm() } else { card.close() }
            }
            Key::Named(Named::Enter) if !card.takes_text() => card.close(),
            _ => vec![],
        };
        Some(self.carry(out))
    }
}
