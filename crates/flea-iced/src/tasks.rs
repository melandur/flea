// The shell's three ways of leaving the UI thread: a sleep, a blocking job answered once, and a
// thumbnail decoded to the icon slot.
use crate::DECODED_CAP;
use iced::widget::image;
use std::time::Duration;

pub(crate) fn sleep(d: Duration) -> impl std::future::Future<Output = ()> {
    let (tx, rx) = iced::futures::channel::oneshot::channel();
    std::thread::spawn(move || {
        std::thread::sleep(d);
        let _ = tx.send(());
    });
    async move {
        let _ = rx.await;
    }
}

// A blocking child off the UI thread, answered once.
pub(crate) fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> impl std::future::Future<Output = T> {
    let (tx, rx) = iced::futures::channel::oneshot::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    async move { rx.await.expect("the worker thread answers before it ends") }
}

// One cache PNG to an icon-slot image. The path is always the backend's own thumbed answer, never a
// filename, and the decode is a PNG decoder alone; AGENTS.md "Deliberate corners" names the residual.
// px is the slot's own decode size: twice the list's icon slot, or the grid's thumbnail size, which is
// what Image.sourceSize is in GridTile.qml.
pub(crate) fn decode(path: &str, px: u32) -> Option<image::Handle> {
    let img = ::image::open(path).ok()?.thumbnail(px, px).to_rgba8();
    let (w, h) = img.dimensions();
    Some(image::Handle::from_rgba(w, h, img.into_raw()))
}

impl crate::Flea {
    // A decoded thumbnail joins the cache, oldest out past the cap; a failed decode drops the entry
    // so the row keeps its mark, as Row.qml's thumbDrawn falls back on Image.Error.
    pub(crate) fn decoded_thumb(&mut self, path: String, handle: Option<image::Handle>) {
                match handle {
            Some(h) => {
                if self.decoded.insert(path.clone(), h).is_none() {
                    self.decoded_order.push_back(path);
                }
                while self.decoded_order.len() > DECODED_CAP {
                    if let Some(old) = self.decoded_order.pop_front() {
                        self.decoded.remove(&old);
                    }
                }
            }
            // An evicted or unreadable cache file: the row keeps its mark, as Row.qml's
            // thumbDrawn falls back on Image.Error.
            None => {
                self.decoded.remove(&path);
            }
        }
    }
}
