// A left click at one point of the compositor's output, through zwlr_virtual_pointer_v1, for
// tests/iced-headless.sh: its private sway has no pointer device, so no window there ever gets a
// pointer, and ydotool would click on the operator's own desktop through uinput instead.
// Usage: test-pointer <output width> <output height> <x> <y>, against $WAYLAND_DISPLAY.
use wayland_client::protocol::{wl_pointer::ButtonState, wl_registry, wl_seat};
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols_wlr::virtual_pointer::v1::client::{zwlr_virtual_pointer_manager_v1 as manager, zwlr_virtual_pointer_v1 as pointer};

#[derive(Default)]
struct State {
    seat: Option<wl_seat::WlSeat>,
    manager: Option<manager::ZwlrVirtualPointerManagerV1>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(state: &mut Self, registry: &wl_registry::WlRegistry, event: wl_registry::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        if let wl_registry::Event::Global { name, interface, .. } = event {
            match interface.as_str() {
                "wl_seat" => state.seat = Some(registry.bind(name, 1, qh, ())),
                "zwlr_virtual_pointer_manager_v1" => state.manager = Some(registry.bind(name, 1, qh, ())),
                _ => {}
            }
        }
    }
}

wayland_client::delegate_noop!(State: ignore wl_seat::WlSeat);
wayland_client::delegate_noop!(State: manager::ZwlrVirtualPointerManagerV1);
wayland_client::delegate_noop!(State: pointer::ZwlrVirtualPointerV1);

// linux/input-event-codes.h BTN_LEFT.
const BTN_LEFT: u32 = 0x110;

fn main() {
    let n: Vec<u32> = std::env::args().skip(1).map(|a| a.parse().expect("four whole numbers")).collect();
    let [w, h, x, y] = n[..] else { panic!("usage: test-pointer <width> <height> <x> <y>") };
    let conn = Connection::connect_to_env().expect("no compositor at WAYLAND_DISPLAY");
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    conn.display().get_registry(&qh, ());
    let mut state = State::default();
    queue.roundtrip(&mut state).unwrap();
    let manager = state.manager.take().expect("the compositor offers no virtual pointer");
    let device = manager.create_virtual_pointer(state.seat.as_ref(), &qh, ());
    let pause = |queue: &mut wayland_client::EventQueue<State>, state: &mut State, ms| {
        queue.roundtrip(state).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(ms));
    };
    // Enter and settle before the press, so the window has the pointer over the target when it lands.
    device.motion_absolute(0, x, y, w, h);
    device.frame();
    pause(&mut queue, &mut state, 300);
    device.button(10, BTN_LEFT, ButtonState::Pressed);
    device.frame();
    pause(&mut queue, &mut state, 60);
    device.button(70, BTN_LEFT, ButtonState::Released);
    device.frame();
    pause(&mut queue, &mut state, 300);
    device.destroy();
    queue.roundtrip(&mut state).unwrap();
}
