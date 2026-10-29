// The search's keys, ui/js/Search.js typeKey, ui/js/SearchKeys.js and ui/js/QueryLine.js: the query
// line owns the keyboard while it is being typed in, an unbound printable key in the listing starts
// it, and on the results a printable key the listing does not bind goes back to the line.
use crate::search::Mode;
use crate::{Flea, Message};
use iced::keyboard::{key::Named, Key, Modifiers};
use iced::Task;

// QueryLine.printable: one character, not a control or DEL, and no Ctrl or Super; Alt is allowed
// because some layouts reach printable characters through it.
fn printable(text: Option<&str>, modifiers: Modifiers) -> Option<String> {
    let t = text?;
    let mut chars = t.chars();
    let c = chars.next()?;
    (chars.next().is_none() && c >= ' ' && c != '\u{7f}' && !modifiers.control() && !modifiers.logo()).then(|| t.to_string())
}

// The listing's own printable bindings in the Default preset: m, the comma and the question mark,
// and Space, which reaches us as a named key.
fn listing_binds(text: &str) -> bool {
    matches!(text, "m" | "," | "?")
}

// QueryLine.oneLine: a paste loses a trailing break, inner breaks and tabs become spaces, other
// control characters go, and it stops at 255 characters, the longest a name can be.
pub fn one_line(text: &str) -> String {
    let t = text.trim_end_matches(['\r', '\n']);
    let mut out = String::new();
    let mut in_run = false;
    for c in t.chars() {
        // /[\r\n\t]+/g: a run of breaks and tabs is one space; literal spaces are kept as they are.
        if matches!(c, '\r' | '\n' | '\t') {
            if !in_run {
                out.push(' ');
            }
            in_run = true;
            continue;
        }
        in_run = false;
        if !c.is_control() {
            out.push(c);
        }
    }
    out.chars().take(255).collect()
}

impl Flea {
    pub(crate) fn search_message(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::SearchLive(generation) => {
                let effects = self.pane.search_live(generation);
                self.run(effects)
            }
            Message::SearchPaste(text) => self.search_paste(text),
            Message::SortColumn(key) => {
                let effects = self.pane.sort_column(key);
                self.run(effects)
            }
            Message::CrumbPressed(i) => self.crumb_pressed(i),
            Message::CrumbSettle(generation) => self.crumb_settled(generation),
            Message::PathInput(text) => {
                self.chrome.field = text;
                Task::none()
            }
            Message::PathSubmit => self.commit_path_edit(),
            _ => Task::none(),
        }
    }

    pub(crate) fn search_key(&mut self, key: &Key, modifiers: Modifiers, text: Option<&str>) -> Option<Task<Message>> {
        let typed = printable(text, modifiers);
        match self.pane.search.mode {
            Mode::Typing => Some(self.query_line_key(key, modifiers, typed)),
            Mode::Results if !self.rail.focused => match key.as_ref() {
                Key::Named(Named::Escape) => {
                    let effects = self.pane.search_cancel();
                    Some(self.run(effects))
                }
                // SearchKeys.resumeKey: Backspace or an unbound printable key types into the line again.
                Key::Named(Named::Backspace) => {
                    self.pane.search.mode = Mode::Typing;
                    let effects = self.pane.search_backspace();
                    Some(self.run(effects))
                }
                _ => match typed.filter(|t| !listing_binds(t)) {
                    Some(t) => {
                        self.pane.search.mode = Mode::Typing;
                        let effects = self.pane.search_typed(&t);
                        Some(self.run(effects))
                    }
                    None => None,
                },
            },
            // SearchKeys.typeAhead: typing in the listing searches this folder.
            Mode::Off if !self.rail.focused && self.share_browser.is_none() => {
                let t = typed.filter(|t| !listing_binds(t) && t != " ")?;
                let mut effects = self.pane.search_start(false);
                effects.extend(self.pane.search_typed(&t));
                Some(self.run(effects))
            }
            _ => None,
        }
    }

    fn query_line_key(&mut self, key: &Key, modifiers: Modifiers, typed: Option<String>) -> Task<Message> {
        let effects = match key.as_ref() {
            Key::Named(Named::Enter) => self.pane.search_run(),
            Key::Named(Named::Escape) => self.pane.search_close(),
            Key::Named(Named::Backspace) => self.pane.search_backspace(),
            Key::Named(Named::Tab) => self.pane.search_toggle_depth(),
            // Neither query line is an editor, so the clipboard is wired by hand (QueryLine.clipKey).
            Key::Character("v") if modifiers.control() => return iced::clipboard::read().map(Message::SearchPaste),
            Key::Named(Named::Insert) if modifiers.shift() => return iced::clipboard::read().map(Message::SearchPaste),
            Key::Character("c") | Key::Named(Named::Insert) if modifiers.control() => {
                return if self.pane.search.query.is_empty() { Task::none() } else { iced::clipboard::write(self.pane.search.query.clone()) };
            }
            // Focus.LEAVES_LINE: a step off the line commits it and then acts on the results.
            Key::Named(Named::ArrowDown | Named::ArrowUp) => {
                let mut out = self.pane.search_run();
                out.extend(self.pane.step(if key.as_ref() == Key::Named(Named::ArrowDown) { 1 } else { -1 }));
                out
            }
            Key::Named(Named::ArrowRight) => match self.pane.search_commit_for_right() {
                Some(out) if out.iter().any(|e| matches!(e, crate::pane::Effect::Message(_))) => out,
                Some(mut out) => {
                    out.extend(self.pane.forward());
                    out
                }
                None => self.pane.forward(),
            },
            _ => match typed {
                Some(t) => self.pane.search_typed(&t),
                None => vec![],
            },
        };
        self.run(effects)
    }

    pub(crate) fn search_paste(&mut self, text: Option<String>) -> Task<Message> {
        let line = one_line(&text.unwrap_or_default());
        if line.is_empty() || self.pane.search.mode != Mode::Typing {
            return Task::none();
        }
        let effects = self.pane.search_typed(&line);
        self.run(effects)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn printable_refuses_del_and_ctrl_and_allows_alt() {
        assert_eq!(printable(Some("a"), Modifiers::empty()), Some("a".into()));
        assert_eq!(printable(Some("\u{7f}"), Modifiers::empty()), None);
        assert_eq!(printable(Some("a"), Modifiers::CTRL), None);
        assert_eq!(printable(Some("@"), Modifiers::ALT), Some("@".into()));
        assert_eq!(printable(Some("ab"), Modifiers::empty()), None);
    }

    #[test]
    fn a_paste_is_one_line_of_a_names_length() {
        assert_eq!(one_line("report\n"), "report");
        assert_eq!(one_line("a\nb\tc"), "a b c");
        assert_eq!(one_line("  lead"), "  lead");
        assert_eq!(one_line("a  b\n\n\tc\u{7}"), "a  b c");
        assert_eq!(one_line(&"x".repeat(300)).len(), 255);
    }
}
