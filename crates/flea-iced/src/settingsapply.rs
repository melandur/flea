// A saved setting applied to the window that saved it: the merged document `flea --ui-state` printed
// is read with the same Prefs parse boot uses, compared against the settings in force, and only what
// changed is applied, to both panes when the window is split.
use crate::prefs::Prefs;
use crate::wire::Order;
use crate::{Flea, Message};
use iced::Task;

impl Flea {
    pub(crate) fn apply_settings(&mut self, _old: &serde_json::Value) -> Task<Message> {
        let next = Prefs::from_json(&self.panel.doc.to_string());
        let was = std::mem::replace(&mut self.prefs, next.clone());
        let order = Order { by: next.sort_by.clone(), desc: next.sort_desc, folders_first: next.folders_first, group_by_kind: next.group_by_kind };
        let relist = was.hidden != next.hidden || was.sort_by != next.sort_by || was.sort_desc != next.sort_desc
            || was.folders_first != next.folders_first || was.group_by_kind != next.group_by_kind;
        let apply = |f: &mut Flea| {
            f.pane.wrap_at_ends = next.wrap_at_ends;
            f.pane.thumb_mode = next.thumbnails.clone();
            f.pane.file_type_colours = next.file_type_colours;
            if was.thumbnails != next.thumbnails {
                // Rows asked under the old mode would never be asked again under the new one.
                f.pane.thumbs.clear();
            }
            if !relist {
                return Task::none();
            }
            // A Settings change drops the order chosen this session (ui/Backend.qml sortPreference).
            f.pane.hidden = next.hidden;
            f.pane.order = order.clone();
            let effects = f.pane.reread();
            f.run(effects)
        };
        let mine = apply(self);
        let theirs = match self.other.as_ref().map(|o| o.side) {
            Some(side) => self.with_side(side, apply),
            None => Task::none(),
        };
        // A new thumbnail size changes the grid's tiles and the size held thumbnails decode at.
        let resized = if was.thumb_size != next.thumb_size && self.grid_mode() {
            let files: Vec<String> = self.pane.thumbs.keys().filter_map(|i| self.pane.thumb_file(*i).map(String::from)).collect();
            let decodes: Vec<Task<Message>> = files.into_iter().map(|f| self.decode_task(f)).collect();
            Task::batch([self.report_viewport(), Task::batch(decodes)])
        } else {
            Task::none()
        };
        // A new density moves every row, so the rows on screen are counted again from the scroll.
        let redrawn = if was.density != next.density { self.report_viewport() } else { Task::none() };
        if was.theme_id != next.theme_id {
            self.palette = crate::theme::Palette::load(&next.theme_id);
        }
        self.rail.show_trash = next.show_trash;
        self.rail.show_devices = next.show_devices;
        self.rail.show_network = next.show_network;
        self.rail.width = crate::rail::width(next.sidebar_width);
        self.rail.favourites = next.favourites.clone();
        if was.rail_shown != next.rail_shown {
            self.rail.shown = next.rail_shown;
            if !self.rail.shown {
                self.rail.focused = false;
            }
        }
        self.devices.show_unmounted = next.show_unmounted;
        self.network.remember_passwords = next.remember_passwords;
        self.rebuild_rail();
        Task::batch([mine, theirs, resized, redrawn])
    }
}
