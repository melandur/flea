// The File types card, ui/OpenRuleDialog.qml: a file ending typed in, an application chosen from the
// installed catalogue, and Save writing the whole table. The catalogue is asked once while the panel
// is open, under a request id of the card's own, so a menu action in flight behind the panel can
// never answer it, and it is dropped when the panel closes so a newly installed application is in
// the list next time.
use crate::openrules::{self, App};
use crate::settings::Kind;
use crate::wire::Request;
use crate::{marks, Flea, Message, MARK, PAD_X, ROW_H, TEXT_PX};
use iced::keyboard::{key::Named, Key};
use iced::widget::{button, column, container, mouse_area, opaque, row, scrollable, space, stack, svg, text, text_input, Column, Id};
use iced::{Border, Color, Element, Length, Task};

pub const ENDING_ID: &str = "rule-ending";
// viewportRows: how many applications the list shows at once.
const VIEWPORT_ROWS: f32 = 7.0;

#[derive(Debug, Default)]
pub struct Card {
    pub ending: String,
    pub search: String,
    pub cursor: usize,
    pub app: String,
}

#[derive(Debug, Clone)]
pub enum CardMsg {
    Ending(String),
    Search(String),
    Pick(usize),
    Save,
    Cancel,
}

impl Flea {
    fn apps(&self) -> &[App] {
        self.panel.facts.as_ref().map(|f| f.apps.as_slice()).unwrap_or(&[])
    }

    // The section asks when it is shown, and the card asks again if it opens without an answer.
    pub(crate) fn ask_catalogue(&mut self) {
        if !self.apps().is_empty() || self.panel.catalogue_asked != 0 {
            return;
        }
        self.panel.requests += 1;
        self.panel.catalogue_asked = self.panel.requests;
        self.panel.catalogue_error.clear();
        if let Some(b) = &self.backend {
            let _ = b.send(&Request::MenuInstalled { id: self.panel.catalogue_asked });
        }
    }

    pub(crate) fn catalogue_answered(&mut self, id: u64, apps: Result<Vec<(String, String)>, String>) {
        if id != self.panel.catalogue_asked {
            return;
        }
        self.panel.catalogue_asked = 0;
        match apps {
            Ok(list) => {
                if let Some(f) = self.panel.facts.as_mut() {
                    f.apps = list.into_iter().map(|(id, label)| App { id, label }).collect();
                }
                // The catalogue lands while the card is up, so the cursor takes the rule's own row now.
                let at = self.card_cursor_for();
                if let Some(c) = self.panel.card.as_mut() {
                    c.cursor = at;
                }
            }
            Err(e) => self.panel.catalogue_error = if e.is_empty() { "The installed applications could not be read.".into() } else { e },
        }
    }

    fn card_cursor_for(&self) -> usize {
        let Some(c) = &self.panel.card else { return 0 };
        openrules::matching(self.apps(), &c.search).iter().position(|a| a.id == c.app).unwrap_or(0)
    }

    pub(crate) fn open_card(&mut self, ending: &str, app: &str) -> Task<Message> {
        self.panel.card = Some(Card { ending: ending.to_string(), search: String::new(), cursor: 0, app: app.to_string() });
        let at = self.card_cursor_for();
        if let Some(c) = self.panel.card.as_mut() {
            c.cursor = at;
        }
        self.ask_catalogue();
        iced::widget::operation::focus(ENDING_ID)
    }

    // A rule row's own mark, and x or Delete on the cursor row: the rule leaves the table.
    pub(crate) fn remove_rule(&mut self, index: usize) -> Task<Message> {
        let rows = self.panel_rows();
        match rows.get(index).filter(|r| r.kind == Kind::OpenRule) {
            Some(r) => {
                let at = r.id.trim_start_matches("openrule:").parse().unwrap_or(usize::MAX);
                let patch = openrules::without(&self.panel.doc, at);
                self.panel_write(patch)
            }
            None => Task::none(),
        }
    }

    pub(crate) fn card_message(&mut self, msg: CardMsg) -> Task<Message> {
        let Some(c) = self.panel.card.as_mut() else { return Task::none() };
        match msg {
            CardMsg::Ending(t) => c.ending = t,
            CardMsg::Search(t) => {
                c.search = t;
                c.cursor = 0;
            }
            CardMsg::Pick(i) => c.cursor = i,
            CardMsg::Save => return self.card_save(),
            CardMsg::Cancel => self.panel.card = None,
        }
        Task::none()
    }

    // canSubmit: a valid ending and an application under the cursor; nothing is written otherwise.
    fn card_save(&mut self) -> Task<Message> {
        let Some(c) = &self.panel.card else { return Task::none() };
        let matches = openrules::matching(self.apps(), &c.search);
        let Some(app) = matches.get(c.cursor).map(|a| a.id.clone()) else { return Task::none() };
        let Some(patch) = openrules::with_rule(&self.panel.doc, &c.ending, &app) else { return Task::none() };
        self.panel.card = None;
        self.panel_write(patch)
    }

    pub(crate) fn card_key(&mut self, key: &Key) -> Task<Message> {
        let count = self.panel.card.as_ref().map(|c| openrules::matching(self.apps(), &c.search).len()).unwrap_or(0);
        let Some(c) = self.panel.card.as_mut() else { return Task::none() };
        match key.as_ref() {
            Key::Named(Named::Escape) => {
                self.panel.card = None;
                Task::none()
            }
            Key::Named(Named::ArrowDown) => {
                c.cursor = (c.cursor + 1).min(count.saturating_sub(1));
                Task::none()
            }
            Key::Named(Named::ArrowUp) => {
                c.cursor = c.cursor.saturating_sub(1);
                Task::none()
            }
            Key::Named(Named::Tab) => iced::widget::operation::focus_next(),
            Key::Named(Named::Enter) => self.card_save(),
            _ => Task::none(),
        }
    }

    pub(crate) fn card_layer<'a>(&'a self, base: Element<'a, Message>) -> Element<'a, Message> {
        let Some(c) = &self.panel.card else { return base };
        let p = self.palette;
        let matches = openrules::matching(self.apps(), &c.search);
        let ending = openrules::normalised(&c.ending);
        let can_save = openrules::is_ending(&ending) && matches.get(c.cursor).is_some();
        let mut list = Column::new();
        for (i, a) in matches.iter().enumerate() {
            let here = i == c.cursor;
            let band = container(row![svg(marks::handle("app-window", p.dim)).width(MARK).height(MARK), text(a.label.clone())].spacing(10).align_y(iced::Center))
                .padding([0.0, 10.0])
                .height(ROW_H)
                .width(Length::Fill)
                .align_y(iced::Center)
                .style(move |_| if here { container::Style::default().background(Color { a: 0.18, ..p.accent }) } else { container::Style::default() });
            list = list.push(mouse_area(band).on_press(Message::Card(CardMsg::Pick(i))));
        }
        let status: Element<'a, Message> = if !self.panel.catalogue_error.is_empty() {
            text(self.panel.catalogue_error.clone()).color(p.error).into()
        } else if self.apps().is_empty() {
            text("Reading the installed applications…").color(p.muted).into()
        } else {
            space().height(0).into()
        };
        let body = column![
            text("FILE ENDING").size(11.0).color(p.muted),
            text_input(".nii.gz", &c.ending).id(Id::new(ENDING_ID)).on_input(|t| Message::Card(CardMsg::Ending(t))).on_submit(Message::Card(CardMsg::Save)).size(TEXT_PX),
            text_input("Search applications", &c.search).on_input(|t| Message::Card(CardMsg::Search(t))).on_submit(Message::Card(CardMsg::Save)).size(TEXT_PX),
            container(scrollable(list)).height(ROW_H * VIEWPORT_ROWS),
            status,
            row![
                space().width(Length::Fill),
                button(text("Cancel")).on_press(Message::Card(CardMsg::Cancel)),
                button(text("Save")).on_press_maybe(can_save.then_some(Message::Card(CardMsg::Save))),
            ]
            .spacing(8),
        ]
        .spacing(10);
        let card = container(body)
            .padding(PAD_X + 2.0)
            .width(420)
            .style(move |_| container::Style::default().background(p.background).border(Border { color: p.muted, width: 1.0, radius: 8.0.into() }));
        stack![base, container(opaque(card)).center(Length::Fill)].into()
    }
}
