// What a Places or Favorites row's menu does, ui/js/PlaceMenu.js perform: every row takes the path
// the menu was opened on, never the listing's cursor. A favourite is removed by index, so the index
// is checked against that path first: the rail rebuilds on its poll, and Favorites can be rewritten
// by the other front end in between. Properties is not here, because a place has no row for the
// backend's menuaction op to snapshot, which is QML's own reason.
use crate::menushell::Msg;
use crate::{blocking, Flea, Message};
use iced::Task;

impl Flea {
    pub(crate) fn place_act(&mut self, action: &str, path: String, favourite: Option<usize>) -> Task<Message> {
        match action {
            "open" => {
                let effects = self.pane.open(path, None);
                self.run(effects)
            }
            "openTab" => self.open_tab_at(path),
            "openTerminal" => self.open_terminal_at(path),
            "copypath" => self.copy_text(path),
            "addFavourite" => self.favourite_add(path),
            "removeFavourite" => self.favourite_remove(favourite, &path),
            _ => Task::none(),
        }
    }

    fn favourite_remove(&mut self, index: Option<usize>, path: &str) -> Task<Message> {
        let records = self.rail.favourites.clone();
        let Some(index) = index.filter(|i| records.get(*i).and_then(|r| r.get("path")).and_then(|p| p.as_str()) == Some(path)) else {
            return self.show("Favorites changed; reopen the menu before removing this row.".into(), true);
        };
        let op = serde_json::json!({ "op": "remove", "index": index, "expected": records }).to_string();
        let bin = crate::backend::binary();
        Task::perform(blocking(move || crate::dialog::add_favourite(&bin, &op)), |r| Message::Menu(Msg::Favourite(r)))
    }
}
