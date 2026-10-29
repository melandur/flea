// The key sheet, as ui/KeymapSheet.qml draws ui/js/Keymap.js sheetFor("default", "gui"): the four
// groups keys.toml's [sheetgroups] names, in its order, each row the best spelling of a key and its
// wording. ? opens it, any key closes it: it is a reference and not a mode.
//
// The rows are sheetFor's own answer for the Default preset, which is the only preset since the key
// cut, read out of the QML's generated table by running that function; a test below checks the
// groups, their order and every base label against ui/js/Keymap.js, so a regenerated table that
// moves a row reddens here rather than drifting.
use crate::{Flea, Message, PAD_X, TEXT_PX};
use iced::widget::{column, container, mouse_area, opaque, row, space, stack, text, Column};
use iced::{Border, Color, Element, Length};

// (group, action, keys, label)
pub const SHEET: &[(&str, &str, &str, &str)] = &[
    ("move", "cursorDown", "down", "move"),
    ("move", "cursorUp", "up", "cursor up"),
    ("move", "open", "enter", "open"),
    ("move", "parent", "left", "parent"),
    ("move", "pageForward", "right", "open / preview"),
    ("move", "focusNext", "tab", "scope / focus"),
    ("move", "escape", "escape", "escape"),
    ("move", "toggleSelect", "space", "select"),
    ("move", "selectAll", "ctrl-a", "select all"),
    ("look", "preview", "space", "preview"),
    ("look", "sidebar", "ctrl-g", "sidebar"),
    ("look", "toggleDual", "ctrl-t", "toggle dual"),
    ("look", "toggleHidden", "ctrl-h", "toggle hidden"),
    ("look", "mute", "m", "mute"),
    ("look", "keymapSheet", "?", "keys"),
    ("look", "refresh", "f5", "reload"),
    ("find", "search", "ctrl-f", "find / deep"),
    ("find", "searchDeep", "ctrl-shift-f", "search deep"),
    ("find", "sortName", "ctrl-j", "sort"),
    ("find", "sortSize", "ctrl-k", "sort size"),
    ("find", "sortDate", "ctrl-l", "sort date"),
    ("change", "copy", "ctrl-c", "copy"),
    ("change", "cut", "ctrl-x", "cut"),
    ("change", "paste", "ctrl-v", "paste"),
    ("change", "copypath", "ctrl-b", "file path"),
    ("change", "rename", "f2", "rename"),
    ("change", "trash", "delete", "trash"),
    ("change", "undo", "ctrl-z", "undo"),
    ("change", "newFolder", "ctrl-n", "new folder"),
    ("change", "newFile", "ctrl-m", "new file"),
    ("change", "menu", "m / menu", "menu"),
    ("change", "settings", ",", "settings"),
];

// sheetFor(dual): in the split, Tab is what moves between the panes.
pub fn row_for(action: &str, keys: &'static str, label: &'static str, dual: bool) -> (&'static str, &'static str) {
    if dual && action == "focusNext" { ("tab", "focus other pane") } else { (keys, label) }
}

impl Flea {
    pub(crate) fn sheet_layer<'a>(&'a self, base: Element<'a, Message>) -> Element<'a, Message> {
        if !self.sheet_open {
            return base;
        }
        let p = self.palette;
        let dual = self.other.is_some();
        let group = |name: &str| -> Element<'a, Message> {
            let mut col = Column::new().spacing(2);
            let heading = format!("{}{}", name[..1].to_uppercase(), &name[1..]);
            col = col.push(text(heading).size(11.0).color(p.muted));
            for &(_, action, keys, label) in SHEET.iter().filter(|r| r.0 == name) {
                let (keys, label) = row_for(action, keys, label, dual);
                let cap = container(text(keys).size(11.0).color(p.foreground))
                    .padding([1.0, 6.0])
                    .style(move |_| container::Style::default().border(Border { color: p.muted, width: 1.0, radius: 4.0.into() }));
                col = col.push(row![container(cap).width(110), text(label).size(TEXT_PX).color(p.foreground)].spacing(8).align_y(iced::Center));
            }
            col.into()
        };
        // Two columns of the card, the board's own split: where the cursor goes and what is drawn,
        // beside what narrows and what writes.
        let body = column![
            row![text("Keys").color(p.foreground), space().width(Length::Fill), text("esc closes").size(11.0).color(p.muted)],
            row![column![group("move"), group("look")].spacing(12).width(Length::Fill), column![group("find"), group("change")].spacing(12).width(Length::Fill)].spacing(16),
        ]
        .spacing(12);
        let card = container(body)
            .padding(PAD_X + 2.0)
            .width(520)
            .style(move |_| container::Style::default().background(p.surface).border(Border { color: p.muted, width: 1.0, radius: 8.0.into() }));
        let backdrop = mouse_area(container(space()).width(Length::Fill).height(Length::Fill).style(move |_| {
            container::Style::default().background(Color { a: 0.5, ..p.background })
        }))
        .on_press(Message::Chrome(crate::chrome::Act::SheetClose))
        .on_right_press(Message::Chrome(crate::chrome::Act::SheetClose));
        stack![base, opaque(backdrop), container(opaque(card)).center(Length::Fill)].into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keymap_line(name: &str) -> serde_json::Value {
        let js = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../ui/js/Keymap.js")).unwrap();
        let line = js.lines().find(|l| l.starts_with(&format!("var {name} = "))).unwrap();
        serde_json::from_str(line.trim_start_matches(&format!("var {name} = "))).unwrap()
    }

    #[test]
    fn the_groups_and_their_order_are_keys_toml_sheetgroups() {
        let groups = keymap_line("SHEET_GROUPS");
        let ours: Vec<(&str, &str)> = SHEET.iter().map(|r| (r.0, r.1)).collect();
        let theirs: Vec<(String, String)> = ["move", "look", "find", "change"]
            .iter()
            .flat_map(|g| groups[g].as_array().unwrap().iter().map(move |a| (g.to_string(), a.as_str().unwrap().to_string())))
            .collect();
        assert_eq!(ours.iter().map(|(g, a)| (g.to_string(), a.to_string())).collect::<Vec<_>>(), theirs);
    }

    #[test]
    fn every_label_the_base_sheet_names_is_its_own() {
        for base in keymap_line("BASE_SHEET").as_array().unwrap() {
            let (action, label) = (base["action"].as_str().unwrap(), base["label"].as_str().unwrap());
            let ours = SHEET.iter().find(|r| r.1 == action).unwrap_or_else(|| panic!("{action} is not on the sheet"));
            assert_eq!(ours.3, label, "{action}");
        }
        assert_eq!(row_for("focusNext", "tab", "scope / focus", true), ("tab", "focus other pane"));
    }
}
