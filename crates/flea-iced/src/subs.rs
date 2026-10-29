// What the window listens to besides its own messages: each backend child's lines, the keyboard,
// the mouse's back button, the close and resize events, and media playback's tick. Out of main.rs whole.
use crate::{backend, chrome, keys, Flea, Message};
use iced::{keyboard, window, Subscription};

impl Flea {
    pub(crate) fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            // One stream per live child, each tagged with the spawn key that names its pane.
            Subscription::batch(
                std::iter::once(self.backend_key)
                    .chain(self.other.as_ref().map(|o| o.key))
                    .map(|key| Subscription::run_with(key, backend::events).with(key).map(|(key, event)| Message::Backend(key, event))),
            ),
            keyboard::listen().map(Message::Key),
            iced::event::listen_with(keys::captured_escape).map(Message::Key),
            iced::event::listen_with(chrome::mouse_back).map(Message::Chrome),
            window::close_requests().map(|_| Message::CloseRequested),
            window::resize_events().map(|(_, size)| Message::Resized(size)),
            // TrashView's 2.5 s check while a confirmation is open: a nested change GIO's monitor misses.
            if self.trash.confirmation.is_some() {
                iced::time::every(std::time::Duration::from_millis(2500)).map(|_| Message::Trash(crate::trashview::Msg::Tick))
            } else {
                Subscription::none()
            },
            // A playing file is drawn at its own pace, about 30 frames a second; a paused one is
            // only polled for its bus, and nothing ticks at all without media open.
            match self.playback.media.as_ref().map(|m| m.playing).or(self.column_media_playing()) {
                Some(true) => iced::time::every(std::time::Duration::from_millis(33)).map(|_| Message::MediaTick),
                Some(false) => iced::time::every(std::time::Duration::from_millis(250)).map(|_| Message::MediaTick),
                None => Subscription::none(),
            },
        ])
    }
}
