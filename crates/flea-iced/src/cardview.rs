// The menu's dialog cards, as ui/MenuActionDialog.qml, ui/TrashConfirm.qml and ui/PermissionsDialog.qml
// draw them: a title over a hairline, the field or the facts, one line of status or refusal, and the
// buttons, over a backdrop whose press is the card's Cancel. Permissions draws the owner, group and
// everyone rows of check boxes beside the octal field they edit, and the change it would make.
use crate::actioncard::{octal, parse_mode, title, Card, Kind, Phase};
use crate::cardshell::{Msg, ACTION_FIELD};
use crate::{format, Flea, Message, PAD_X, TEXT_PX};
use iced::widget::{button, checkbox, column, container, mouse_area, opaque, row, space, stack, text, text_input, Column, Id};
use iced::{Border, Color, Element, Length};
use serde_json::Value;

impl Flea {
    pub(crate) fn action_layer<'a>(&'a self, base: Element<'a, Message>) -> Element<'a, Message> {
        let Some(c) = self.card.as_ref().filter(|c| c.visible()) else { return base };
        let p = self.palette;
        let backdrop = mouse_area(container(space()).width(Length::Fill).height(Length::Fill).style(move |_| {
            container::Style::default().background(Color { a: 0.5, ..p.background })
        }))
        .on_press(Message::Action(Msg::Close))
        .on_right_press(Message::Action(Msg::Close));
        stack![base, opaque(backdrop), container(opaque(self.card_body(c))).center(Length::Fill)].into()
    }

    fn card_body<'a>(&'a self, c: &'a Card) -> Element<'a, Message> {
        let p = self.palette;
        let rule = || container(space().width(Length::Fill).height(1)).style(move |_| container::Style::default().background(Color { a: 0.4, ..p.muted }));
        let mut body = Column::new().spacing(10);
        if c.phase == Phase::Confirm {
            let n = c.facts.get("count").and_then(Value::as_u64).unwrap_or(0);
            body = body
                .push(text(format!("Delete {n} {} permanently?", if n == 1 { "item" } else { "items" })).color(p.foreground))
                .push(text("These items are deleted from disk. This cannot be undone.").color(p.foreground));
            body = body.push(row![space().width(Length::Fill), self.card_button("Cancel", c.focus == 0, false, Some(Msg::Close)), self.card_button("Delete", c.focus == 1, true, Some(Msg::Confirm))].spacing(8));
            return self.card_frame(body, 420.0);
        }
        body = body.push(text(title(c.kind)).color(p.foreground)).push(rule());
        match c.kind {
            Kind::NewFile | Kind::MoveTo | Kind::CopyTo => {
                let label = if c.kind == Kind::NewFile { "Name" } else { "Destination folder" };
                let field = text_input("", &c.text)
                    .id(Id::new(ACTION_FIELD))
                    .on_input_maybe((c.phase == Phase::Ready).then_some(|s| Message::Action(Msg::Input(s))))
                    .on_submit(Message::Action(Msg::Submit))
                    .size(TEXT_PX);
                body = body.push(text(label).color(p.foreground)).push(field);
            }
            Kind::Properties => body = body.push(self.properties(&c.facts)),
            Kind::Permissions => body = body.push(self.permissions(c)),
            Kind::Delete => {}
        }
        let status = if !c.error.is_empty() {
            Some((c.error.clone(), p.error))
        } else if c.kind == Kind::Permissions && !c.reason().is_empty() {
            Some((c.reason(), p.error))
        } else if c.phase != Phase::Ready {
            Some(((if c.kind == Kind::Permissions && !c.facts.is_null() { "Applying permissions…" } else { "Working..." }).to_string(), p.muted))
        } else if c.kind == Kind::Permissions && c.can_edit_mode() && parse_mode(&c.text).is_none() {
            Some(("Enter three octal digits or a leading-zero four-digit mode.".to_string(), p.error))
        } else {
            None
        };
        if let Some((line, ink)) = status {
            body = body.push(text(line).size(11.0).color(ink));
        }
        let close_label = if c.kind == Kind::Properties { "Close" } else { "Cancel" };
        let mut buttons = row![space().width(Length::Fill), self.card_button(close_label, !c.can_submit(), false, Some(Msg::Close))].spacing(8);
        let submit = match c.kind {
            Kind::NewFile => Some("Create"),
            Kind::MoveTo => Some("Move"),
            Kind::CopyTo => Some("Copy"),
            Kind::Permissions => Some("Apply"),
            _ => None,
        };
        if let Some(label) = submit {
            buttons = buttons.push(self.card_button(label, c.can_submit(), false, c.can_submit().then_some(Msg::Submit)));
        }
        self.card_frame(body.push(buttons), if c.kind == Kind::Permissions { 440.0 } else { 420.0 })
    }

    fn card_frame<'a>(&self, body: Column<'a, Message>, width: f32) -> Element<'a, Message> {
        let p = self.palette;
        container(body)
            .padding(PAD_X + 2.0)
            .width(width)
            .style(move |_| container::Style::default().background(p.surface).border(Border { color: p.muted, width: 1.0, radius: 8.0.into() }))
            .into()
    }

    // DialogButton: the focused one is primary, a destructive one draws in the error ink.
    fn card_button<'a>(&self, label: &'a str, primary: bool, danger: bool, msg: Option<Msg>) -> Element<'a, Message> {
        let p = self.palette;
        let tone = if danger { p.error } else { p.accent };
        button(text(label).size(TEXT_PX))
            .on_press_maybe(msg.map(Message::Action))
            .style(move |_, _| button::Style {
                background: Some(if primary { Color { a: 0.25, ..tone } } else { p.background }.into()),
                text_color: if danger { p.error } else { p.foreground },
                border: Border { color: if primary { tone } else { p.muted }, width: 1.0, radius: 4.0.into() },
                ..Default::default()
            })
            .into()
    }

    fn properties<'a>(&self, f: &Value) -> Element<'a, Message> {
        let p = self.palette;
        if f.get("ok") != Some(&Value::Bool(true)) {
            return column![].into();
        }
        let s = |k: &str| f.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let n = |k: &str| f.get(k).and_then(Value::as_u64).unwrap_or(0);
        let owner = if s("owner").is_empty() { "Unknown".into() } else { s("owner") };
        let mut facts = vec![
            ("Path", s("path")),
            ("Type", s("kind")),
            ("Size", format!("{} ({} bytes)", format::size(n("bytes")), n("bytes"))),
            ("Modified", format::date(n("modified") as i64)),
            ("Permissions", s("mode")),
            ("Owner", format!("{owner} (uid {})", n("uid"))),
            ("Group", format!("gid {}", n("gid"))),
        ];
        if f.get("symlink") == Some(&Value::Bool(true)) {
            facts.push(("Link target", s("target")));
        }
        Column::with_children(facts.into_iter().map(|(label, value)| {
            column![text(label).size(11.0).color(p.muted), text(value).color(p.foreground).wrapping(text::Wrapping::Glyph)].into()
        }))
        .spacing(6)
        .into()
    }

    fn permissions<'a>(&self, c: &'a Card) -> Element<'a, Message> {
        let p = self.palette;
        let f = &c.facts;
        let directory = f.get("directory") == Some(&Value::Bool(true));
        let mode = parse_mode(&c.text).or_else(|| f.get("mode").and_then(Value::as_str).and_then(parse_mode)).unwrap_or(0);
        let leaf = c.path.rsplit('/').next().unwrap_or("").to_string();
        let mut col = Column::new().spacing(8);
        col = col.push(row![text(leaf).color(p.foreground).width(Length::Fill), text(if f.is_null() { "" } else if directory { "directory" } else { "file" }).size(11.0)]);
        let heads = ["READ", "WRITE", if directory { "ENTER" } else { "EXEC" }];
        col = col.push(row![space().width(96)].extend(heads.iter().map(|h| text(*h).size(11.0).width(Length::Fill).into())));
        for (r, who) in ["Owner", "Group", "Everyone"].iter().enumerate() {
            let mut line = row![text(*who).width(96)];
            for k in 0..3 {
                let bit = 1u32 << (8 - r * 3 - k);
                let check = checkbox(mode & bit != 0).on_toggle_maybe(c.can_edit_mode().then_some(move |_| Message::Action(Msg::Bit(bit))));
                line = line.push(container(check).width(Length::Fill));
            }
            col = col.push(line.align_y(iced::Center));
        }
        let field = text_input("", &c.text)
            .id(Id::new(ACTION_FIELD))
            .on_input_maybe(c.can_edit_mode().then_some(|s| Message::Action(Msg::Input(s))))
            .on_submit(Message::Action(Msg::Submit))
            .size(TEXT_PX);
        col = col.push(row![text("Octal").width(96), field].align_y(iced::Center));
        if !f.is_null() {
            let who = |k: &str, id: &str| format!("{} · read-only  {id} {}", f.get(k).and_then(Value::as_str).filter(|s| !s.is_empty()).unwrap_or("Unknown"), f.get(id).and_then(Value::as_u64).unwrap_or(0));
            col = col.push(text(format!("Owner  {}", who("owner", "uid"))).size(11.0)).push(text(format!("Group  {}", who("group", "gid"))).size(11.0));
        }
        let requested = if c.can_edit_mode() { parse_mode(&c.text).map(|m| format!("Requested mode {}", octal(m))) } else { None };
        let scope = if directory { "Scope this directory only · enclosed items unchanged · ownership unchanged" } else { "Scope this item only · ownership unchanged" };
        col.push(text("WILL CHANGE").size(11.0).color(p.foreground))
            .push(text(format!("{}\nPath {}", requested.unwrap_or_else(|| "No changes available".into()), c.path)).size(11.0).color(p.accent))
            .push(text(scope).size(11.0).color(p.muted))
            .into()
    }
}
