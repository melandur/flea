// A left click at one point of the compositor's output, through zwlr_virtual_pointer_v1, for
// tests/iced-headless.sh: its private sway has no pointer device, so no window there ever gets a
// pointer, and ydotool would click on the operator's own desktop through uinput instead.
// Usage: test-pointer <output width> <output height> <x> <y> [3 for the right button], against
// $WAYLAND_DISPLAY; or test-pointer <w> <h> <x> <y> 1 <x2> <y2> [hold ms], a left drag from the first
// point to the second in twenty steps, held there before the release, which is the selection band;
// or test-pointer <w> <h> <x> <y> 4 <notches>, that many wheel notches down over the point.
use wayland_client::protocol::{wl_pointer::{Axis, AxisSource, ButtonState}, wl_registry, wl_seat};
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

// linux/input-event-codes.h BTN_LEFT and BTN_RIGHT.
const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;

fn main() {
    let n: Vec<u32> = std::env::args().skip(1).map(|a| a.parse().expect("four whole numbers")).collect();
    let [w, h, x, y] = n[..4.min(n.len())] else { panic!("usage: test-pointer <width> <height> <x> <y> [3]") };
    let button = if n.get(4) == Some(&3) { BTN_RIGHT } else { BTN_LEFT };
    let drag = n.get(5).zip(n.get(6)).map(|(x2, y2)| (*x2, *y2, n.get(7).copied().unwrap_or(0)));
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
    if n.get(4) == Some(&4) {
        for notch in 0..n.get(5).copied().unwrap_or(1) {
            device.axis_source(AxisSource::Wheel);
            device.axis_discrete(10 + notch, Axis::VerticalScroll, 15.0, 1);
            device.frame();
            pause(&mut queue, &mut state, 20);
        }
        pause(&mut queue, &mut state, 300);
        device.destroy();
        queue.roundtrip(&mut state).unwrap();
        return;
    }
    device.button(10, button, ButtonState::Pressed);
    device.frame();
    pause(&mut queue, &mut state, 60);
    let mut time = 70;
    if let Some((x2, y2, hold)) = drag {
        for step in 1..=20i64 {
            let at = |a: u32, b: u32| (a as i64 + (b as i64 - a as i64) * step / 20) as u32;
            time += 20;
            device.motion_absolute(time, at(x, x2), at(y, y2), w, h);
            device.frame();
            pause(&mut queue, &mut state, 20);
        }
        pause(&mut queue, &mut state, hold as u64);
        time += 20 + hold;
    }
    device.button(time, button, ButtonState::Released);
    device.frame();
    pause(&mut queue, &mut state, 300);
    device.destroy();
    queue.roundtrip(&mut state).unwrap();
}
