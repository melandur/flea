// The platform drag, ui/FileDrag.qml's Drag.Automatic and every DropArea, over the Wayland data
// device. winit does no drag and drop on Wayland at all, so this binds its own wl_data_device and
// wl_pointer on the window's own connection, through a second event queue on a thread of its own:
// the pointer only to learn the press serial start_drag must name, the device for both directions.
// A drag of Flea's rows offers Drag.mimeFor's three types and Copy alone; a drag from anywhere comes
// in as Enter, Motion, Leave and Drop, a foreign one with its uri-list and any shelf token already
// read. dragshell.rs decides what each means.
use iced::futures::channel::mpsc::{self, UnboundedReceiver, UnboundedSender};
use std::io::{Read, Write};
use std::os::fd::AsFd;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use wayland_backend::client::{Backend, ObjectId};
use wayland_client::protocol::wl_data_device_manager::{DndAction, WlDataDeviceManager};
use wayland_client::protocol::{wl_data_device, wl_data_offer, wl_data_source, wl_pointer, wl_registry, wl_seat, wl_surface::WlSurface};
use wayland_client::{event_created_child, Connection, Dispatch, Proxy, QueueHandle, WEnum};

pub const ROWS_MIME: &str = "application/x-flea-rows";
pub const SHELF_MIME: &str = "application/x-flea-shelf";
pub const URI_MIME: &str = "text/uri-list";
// How long a foreign source has to hand over its uri-list before the drag reads as carrying none.
const READ_LIMIT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone)]
pub enum Event {
    // A drag entered the window: whether it is this process's own, and a foreign one's local paths.
    Enter { x: f64, y: f64, own: bool, uris: Vec<String>, shelf: Option<String> },
    Motion { x: f64, y: f64 },
    Leave,
    Drop { x: f64, y: f64 },
    // This process's own drag ended, wherever it landed.
    Ended,
}

#[derive(Default)]
struct Shared {
    serial: u32,
    device: Option<wl_data_device::WlDataDevice>,
    manager: Option<WlDataDeviceManager>,
    seat: Option<wl_seat::WlSeat>,
    // The offer the pointer is over, the serial of its enter, and the type it would be taken as.
    offer: Option<(wl_data_offer::WlDataOffer, u32, Option<String>)>,
    source: Option<wl_data_source::WlDataSource>,
    at: (f64, f64),
}

struct Wl {
    shared: Arc<Mutex<Shared>>,
    tx: UnboundedSender<Event>,
}

// What the window holds: the connection to flush after a request, the queue handle new objects
// dispatch on, and the surface a drag starts from.
pub struct Dnd {
    conn: Connection,
    qh: QueueHandle<Wl>,
    surface: WlSurface,
    shared: Arc<Mutex<Shared>>,
}

fn receiver() -> &'static Mutex<Option<UnboundedReceiver<Event>>> {
    static RX: OnceLock<Mutex<Option<UnboundedReceiver<Event>>>> = OnceLock::new();
    RX.get_or_init(|| Mutex::new(None))
}

// Subscription::run's stream: the thread's events, once the window has started it.
pub fn events() -> impl iced::futures::Stream<Item = Event> {
    let rx = receiver().lock().ok().and_then(|mut r| r.take());
    iced::futures::stream::unfold(rx, |rx| async move {
        let mut rx = rx?;
        let next = iced::futures::StreamExt::next(&mut rx).await?;
        Some((next, Some(rx)))
    })
}

impl Dnd {
    // The window's own wl_display and wl_surface, as raw-window-handle hands them over.
    pub fn start(display: usize, surface: usize) -> Option<Dnd> {
        // SAFETY: both pointers are the live window's, from iced's own handle, and outlive the window.
        let backend = unsafe { Backend::from_foreign_display(display as *mut _) };
        let conn = Connection::from_backend(backend);
        let id = unsafe { ObjectId::from_ptr(WlSurface::interface(), surface as *mut _) }.ok()?;
        let surface = WlSurface::from_id(&conn, id).ok()?;
        let mut queue = conn.new_event_queue::<Wl>();
        let qh = queue.handle();
        conn.display().get_registry(&qh, ());
        let (tx, rx) = mpsc::unbounded();
        let shared = Arc::new(Mutex::new(Shared::default()));
        let mut wl = Wl { shared: shared.clone(), tx };
        queue.roundtrip(&mut wl).ok()?;
        queue.roundtrip(&mut wl).ok()?;
        if shared.lock().ok()?.device.is_none() {
            return None;
        }
        *receiver().lock().ok()? = Some(rx);
        // The display is winit's, and it goes away when the window does: dropping this guest backend
        // after that destroys proxies on a freed display, which crashed a closing window. One handle
        // is leaked for the life of the process, and the thread forgets its queue when the
        // connection ends rather than dropping it.
        std::mem::forget(conn.clone());
        std::thread::spawn(move || {
            while queue.blocking_dispatch(&mut wl).is_ok() {}
            std::mem::forget(queue);
            std::mem::forget(wl);
        });
        Some(Dnd { conn, qh, surface, shared })
    }

    // FileDrag.liftBegan: Drag.active, with every payload fixed before it begins.
    pub fn drag(&self, payload: Vec<(String, Vec<u8>)>) -> bool {
        let Ok(mut s) = self.shared.lock() else { return false };
        let (Some(manager), Some(device)) = (s.manager.clone(), s.device.clone()) else { return false };
        if s.serial == 0 {
            return false;
        }
        let source = manager.create_data_source(&self.qh, Arc::new(payload.clone()));
        for (mime, _) in &payload {
            source.offer(mime.clone());
        }
        // Foreign applications see copy only; Chromium reported a move when Move was offered.
        source.set_actions(DndAction::Copy);
        device.start_drag(Some(&source), &self.surface, None, s.serial);
        s.source = Some(source);
        drop(s);
        self.conn.flush().is_ok()
    }

    // Escape on a drag still up: a destroyed source ends it with nothing dropped.
    pub fn cancel(&self) {
        if let Some(source) = self.shared.lock().ok().and_then(|mut s| s.source.take()) {
            source.destroy();
            let _ = self.conn.flush();
        }
    }

    // DropArea's drag.accepted: the offer is taken as `mime` where a drop would land, refused elsewhere.
    pub fn accept(&self, mime: Option<&str>) {
        let Ok(mut s) = self.shared.lock() else { return };
        if let Some((offer, serial, taken)) = s.offer.as_mut() {
            if taken.as_deref() != mime {
                offer.accept(*serial, mime.map(str::to_string));
                *taken = mime.map(str::to_string);
            }
        }
        drop(s);
        let _ = self.conn.flush();
    }

    // drop.accept: the drop was taken, so the offer is finished; a refused one is only let go.
    pub fn finish(&self) {
        if let Some((offer, _, taken)) = self.shared.lock().ok().and_then(|mut s| s.offer.take()) {
            if taken.is_some() {
                offer.finish();
            }
            offer.destroy();
            let _ = self.conn.flush();
        }
    }
}

// One type of an offer, read through a pipe within READ_LIMIT; a source that never answers reads empty.
fn read_offer(conn: &Connection, offer: &wl_data_offer::WlDataOffer, mime: &str) -> String {
    let Ok((mut reader, writer)) = std::io::pipe() else { return String::new() };
    offer.receive(mime.to_string(), writer.as_fd());
    drop(writer);
    let _ = conn.flush();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut text = String::new();
        let _ = reader.read_to_string(&mut text);
        let _ = tx.send(text);
    });
    let deadline = Instant::now() + READ_LIMIT;
    rx.recv_timeout(deadline.saturating_duration_since(Instant::now())).unwrap_or_default()
}

impl Dispatch<wl_registry::WlRegistry, ()> for Wl {
    fn event(state: &mut Self, registry: &wl_registry::WlRegistry, event: wl_registry::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        let wl_registry::Event::Global { name, interface, version } = event else { return };
        let Ok(mut s) = state.shared.lock() else { return };
        match interface.as_str() {
            "wl_seat" if s.seat.is_none() => s.seat = Some(registry.bind(name, version.min(7), qh, ())),
            "wl_data_device_manager" => s.manager = Some(registry.bind(name, version.min(3), qh, ())),
            _ => {}
        }
        if let (Some(seat), Some(manager), None) = (s.seat.clone(), s.manager.clone(), s.device.as_ref()) {
            s.device = Some(manager.get_data_device(&seat, qh, ()));
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for Wl {
    fn event(_: &mut Self, seat: &wl_seat::WlSeat, event: wl_seat::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        if let wl_seat::Event::Capabilities { capabilities: WEnum::Value(c) } = event {
            if c.contains(wl_seat::Capability::Pointer) {
                seat.get_pointer(qh, ());
            }
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, ()> for Wl {
    fn event(state: &mut Self, _: &wl_pointer::WlPointer, event: wl_pointer::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        // The implicit grab's serial: start_drag must name the press that is still held.
        if let wl_pointer::Event::Button { serial, state: WEnum::Value(wl_pointer::ButtonState::Pressed), .. } = event {
            if let Ok(mut s) = state.shared.lock() {
                s.serial = serial;
            }
        }
    }
}

type Mimes = Mutex<Vec<String>>;

impl Dispatch<wl_data_device::WlDataDevice, ()> for Wl {
    fn event(state: &mut Self, _: &wl_data_device::WlDataDevice, event: wl_data_device::Event, _: &(), conn: &Connection, _: &QueueHandle<Self>) {
        use wl_data_device::Event as E;
        match event {
            E::Enter { serial, x, y, id: Some(offer), .. } => {
                let mimes = offer.data::<Mimes>().and_then(|m| m.lock().ok().map(|m| m.clone())).unwrap_or_default();
                offer.set_actions(DndAction::Copy, DndAction::Copy);
                let own = state.shared.lock().is_ok_and(|s| s.source.is_some());
                // This process's own source answers on this very thread, so its data is never read here.
                let has = |m: &str| mimes.iter().any(|x| x == m);
                let uris = if !own && has(URI_MIME) { read_offer(conn, &offer, URI_MIME) } else { String::new() };
                let shelf = (!own && has(SHELF_MIME)).then(|| read_offer(conn, &offer, SHELF_MIME));
                if let Ok(mut s) = state.shared.lock() {
                    s.offer = Some((offer, serial, None));
                    s.at = (x, y);
                }
                let uris = uris.split("\r\n").filter(|u| !u.is_empty()).map(str::to_string).collect();
                let _ = state.tx.unbounded_send(Event::Enter { x, y, own, uris, shelf });
            }
            E::Motion { x, y, .. } => {
                if let Ok(mut s) = state.shared.lock() {
                    s.at = (x, y);
                }
                let _ = state.tx.unbounded_send(Event::Motion { x, y });
            }
            E::Leave => {
                if let Some((offer, ..)) = state.shared.lock().ok().and_then(|mut s| s.offer.take()) {
                    offer.destroy();
                }
                let _ = state.tx.unbounded_send(Event::Leave);
            }
            E::Drop => {
                let (x, y) = state.shared.lock().map(|s| s.at).unwrap_or_default();
                let _ = state.tx.unbounded_send(Event::Drop { x, y });
            }
            // The clipboard's offers come through the same device and are not this module's.
            E::Selection { id: Some(offer) } => offer.destroy(),
            _ => {}
        }
    }

    event_created_child!(Wl, wl_data_device::WlDataDevice, [
        wl_data_device::EVT_DATA_OFFER_OPCODE => (wl_data_offer::WlDataOffer, Mutex::new(Vec::<String>::new())),
    ]);
}

impl Dispatch<wl_data_offer::WlDataOffer, Mimes> for Wl {
    fn event(_: &mut Self, _: &wl_data_offer::WlDataOffer, event: wl_data_offer::Event, mimes: &Mimes, _: &Connection, _: &QueueHandle<Self>) {
        if let wl_data_offer::Event::Offer { mime_type } = event {
            if let Ok(mut m) = mimes.lock() {
                m.push(mime_type);
            }
        }
    }
}

impl Dispatch<wl_data_source::WlDataSource, Arc<Vec<(String, Vec<u8>)>>> for Wl {
    fn event(state: &mut Self, source: &wl_data_source::WlDataSource, event: wl_data_source::Event, payload: &Arc<Vec<(String, Vec<u8>)>>, _: &Connection, _: &QueueHandle<Self>) {
        use wl_data_source::Event as E;
        match event {
            // Written off this thread, so a reader that is slow to drain never stalls the drag.
            E::Send { mime_type, fd } => {
                let bytes = payload.iter().find(|(m, _)| *m == mime_type).map(|(_, b)| b.clone()).unwrap_or_default();
                std::thread::spawn(move || {
                    let _ = std::fs::File::from(fd).write_all(&bytes);
                });
            }
            E::Cancelled | E::DndFinished => {
                source.destroy();
                if let Ok(mut s) = state.shared.lock() {
                    s.source = None;
                }
                let _ = state.tx.unbounded_send(Event::Ended);
            }
            _ => {}
        }
    }
}

wayland_client::delegate_noop!(Wl: ignore WlDataDeviceManager);
