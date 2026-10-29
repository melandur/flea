// The connect dialog's card, ui/NetworkDialog.qml and ui/NetworkForm.qml: the protocol chips, the
// fields each protocol shows, the box for TLS, the URI it mounts as, the status line and two
// buttons. A plain overlay with a backdrop, the same choice ui/ContextMenu.qml made.
use crate::dialog::{Dialog, Field, Msg};
use crate::form::PROTOCOLS;
use crate::{Flea, Message, PAD_X, TEXT_PX};
use iced::widget::{button, checkbox, column, container, mouse_area, opaque, row, space, stack, text, text_input, Id};
use iced::{Border, Color, Element, Length};

pub const HOST_ID: &str = "dialog-host";

impl Flea {
    pub(crate) fn dialog_layer<'a>(&'a self, base: Element<'a, Message>) -> Element<'a, Message> {
        let Some(d) = &self.dialog else { return base };
        // The backdrop swallows the press; Escape and Cancel are the ways out, not a stray click.
        let backdrop = mouse_area(container(space()).width(Length::Fill).height(Length::Fill).style(|_| {
            container::Style::default().background(Color { a: 0.45, ..Color::BLACK })
        }));
        stack![base, opaque(backdrop), container(self.dialog_card(d)).center(Length::Fill)].into()
    }

    fn dialog_card<'a>(&'a self, d: &'a Dialog) -> Element<'a, Message> {
        let p = self.palette;
        let spec = d.form.spec();
        let field = |label: &'a str, value: &'a str, which: Field| -> Element<'a, Message> {
            let mut input = text_input("", value).on_input(move |s| Message::Dialog(Msg::Field(which, s))).on_submit(Message::Dialog(Msg::Submit)).size(TEXT_PX);
            if which == Field::Host {
                input = input.id(Id::new(HOST_ID));
            }
            if which == Field::Password && !d.show_password {
                input = input.secure(true);
            }
            column![text(label).size(11.0).color(p.muted), input].spacing(4).into()
        };
        let chips = row(PROTOCOLS.iter().map(|name| {
            let picked = d.form.protocol == *name;
            button(text(*name).size(TEXT_PX))
                .on_press(Message::Dialog(Msg::Pick(name)))
                .style(move |_, _| button::Style {
                    background: Some(if picked { Color { a: 0.25, ..p.accent } } else { p.surface }.into()),
                    text_color: p.foreground,
                    border: Border { color: if picked { p.accent } else { p.muted }, width: 1.0, radius: 4.0.into() },
                    ..Default::default()
                })
                .into()
        }))
        .spacing(6);
        let mut body = column![
            text("ADD NETWORK LOCATION").size(11.0).color(p.muted),
            text(d.title()).color(p.foreground),
            chips,
            field("Label", &d.form.label, Field::Label),
            row![
                container(field("Host", &d.form.host, Field::Host)).width(Length::FillPortion(3)),
                container(field("Port", &d.form.port, Field::Port)).width(Length::FillPortion(1)),
            ]
            .spacing(8),
            field(spec.path_label, &d.form.path, Field::Path),
        ]
        .spacing(10);
        if spec.domain {
            body = body.push(field("Domain", &d.form.domain, Field::Domain));
        }
        if spec.credentials {
            body = body.push(field("Username", &d.form.user, Field::User));
            let eye = button(text(if d.show_password { "Hide" } else { "Show" }).size(11.0)).on_press(Message::Dialog(Msg::Eye));
            body = body.push(row![container(field("Password", &d.form.password, Field::Password)).width(Length::Fill), eye].spacing(8).align_y(iced::Bottom));
        } else {
            body = body.push(text("No credentials: NFS trusts the client host").size(11.0).color(p.muted));
        }
        if spec.tls {
            body = body.push(checkbox(d.form.tls).label("Encrypted (TLS)").on_toggle(|on| Message::Dialog(Msg::Tls(on))));
        }
        let uri = d.form.uri();
        body = body.push(row![text("Mounts as").size(11.0).color(p.muted), text(if uri.is_empty() { "--".to_string() } else { uri }).size(11.0)].spacing(8));
        if !d.status.is_empty() {
            // A question is not a failure, so it reads in the foreground and a failure in the error ink.
            let ink = if d.verifying || !d.failed_connect && !d.status.starts_with("Connect failed:") { p.foreground } else { p.error };
            body = body.push(text(d.status.clone()).color(ink));
        }
        let action = button(text(d.action())).on_press_maybe((!d.busy()).then_some(Message::Dialog(Msg::Submit)));
        let cancel = button(text("Cancel")).on_press_maybe((!d.saving).then_some(Message::Dialog(Msg::Close)));
        body = body.push(row![space().width(Length::Fill), cancel, action].spacing(8));
        container(body)
            .padding(PAD_X + 2.0)
            .width(460)
            .style(move |_| container::Style::default().background(p.background).border(Border { color: p.muted, width: 1.0, radius: 8.0.into() }))
            .into()
    }
}
