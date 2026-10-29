// The shell's three ways of leaving the UI thread: a sleep, a blocking job answered once, and a
// thumbnail decoded to the icon slot.
use crate::THUMB_PX;
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
pub(crate) fn decode(path: &str) -> Option<image::Handle> {
    let img = ::image::open(path).ok()?.thumbnail(THUMB_PX, THUMB_PX).to_rgba8();
    let (w, h) = img.dimensions();
    Some(image::Handle::from_rgba(w, h, img.into_raw()))
}
