// The selection band, ui/SelectionBand.qml: a press on the ground the list or the grid draws no row
// on, then a drag past the platform's drag distance, marks every cell the rectangle crosses. The
// widget lies over the scrollable and tracks the pointer itself, so a drag costs a message only when
// the covered cells change, never one per motion; it draws the rectangle, scrolls while the pointer
// is past the list's top or bottom edge, and ends on the release. A press on a row passes through to
// the row. marquee.rs holds what the band does to the marks.
use crate::marquee::{self, Cells};
use crate::{list_id, Flea, Message};
use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{tree, Tree, Widget};
use iced::advanced::{mouse, renderer, Clipboard, Shell};
use iced::time::Instant;
use iced::widget::{operation, scrollable};
use iced::{Border, Color, Element, Event, Length, Point, Rectangle, Size, Task};

// Qt's startDragDistance, the default styleHints answer, in logical pixels.
const DRAG_DISTANCE: f32 = 10.0;
// SelectionBand's scrollRate: one accelerated list-wheel notch a second, Scroll.distance(0, 120,
// 3 lines, notchPx 24, multiplier 4), with frame time keeping the rate at every refresh rate.
const SCROLL_PX_PER_S: f32 = 3.0 * 24.0 * 4.0;
// Style.selectionFillAlpha's default.
const FILL_ALPHA: f32 = 0.35;

#[derive(Debug, Clone)]
pub enum Ev {
    Press,
    Move { cells: Option<Cells>, columns: usize, back_x: bool, back_y: bool },
    End,
    Cancel,
    Scroll(f32),
}

// What the band is laid over, every field read fresh from the pane at each rebuild.
pub(crate) struct Band {
    side: u8,
    columns: usize,
    // None is the list: one column the width of the band.
    cell_w: Option<f32>,
    cell_h: f32,
    count: usize,
    scroll_y: f32,
    epoch: u64,
    enabled: bool,
    accent: Color,
}

// The press and the sweep it became, kept across rebuilds in the widget tree.
#[derive(Default)]
struct State {
    // The press in window coordinates and the anchor in content coordinates.
    press: Option<(Point, Point)>,
    begun: bool,
    at: Point,
    cells: Option<Cells>,
    epoch: u64,
    // What the cells were measured against: a change in any of them cancels, as SelectionBand's
    // onColumnsChanged, onCellWidthChanged, onCountChanged and the size handlers do.
    shape: (usize, u32, u32, usize, u32, u32),
    frame: Option<Instant>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn band<'a>(side: u8, columns: usize, cell_w: Option<f32>, cell_h: f32, count: usize, scroll_y: f32, epoch: u64, enabled: bool, accent: Color) -> Element<'a, Message> {
    Element::new(Band { side, columns: columns.max(1), cell_w, cell_h, count, scroll_y, epoch, enabled, accent })
}

impl Band {
    fn cell_w(&self, bounds: Rectangle) -> f32 {
        self.cell_w.unwrap_or(bounds.width)
    }

    fn shape(&self, bounds: Rectangle) -> (usize, u32, u32, usize, u32, u32) {
        (self.columns, self.cell_w(bounds).to_bits(), self.cell_h.to_bits(), self.count, bounds.width.to_bits(), bounds.height.to_bits())
    }

    // SelectionBand.endpoint: the pointer clamped into the band, in content coordinates.
    fn end(&self, bounds: Rectangle, at: Point) -> Point {
        Point::new((at.x - bounds.x).clamp(0.0, bounds.width), (at.y - bounds.y).clamp(0.0, bounds.height) + self.scroll_y)
    }

    // SelectionBand.update: a message only when the covered cells changed.
    fn update_cells(&self, state: &mut State, bounds: Rectangle, shell: &mut Shell<'_, Message>) {
        let Some((_, anchor)) = state.press else { return };
        let end = self.end(bounds, state.at);
        let cells = marquee::cells(anchor.x, anchor.y, end.x, end.y, self.cell_w(bounds), self.cell_h, self.columns, self.count);
        if cells != state.cells || !state.begun {
            state.begun = true;
            state.cells = cells;
            shell.publish(Message::Band(self.side, Ev::Move { cells, columns: self.columns, back_x: end.x < anchor.x, back_y: end.y < anchor.y }));
        }
    }

    fn reset(state: &mut State) {
        *state = State { epoch: state.epoch, ..State::default() };
    }

    // SelectionBand.onPressed: indexAt answers -1 on the ground under or beside the cells.
    fn on_ground(&self, bounds: Rectangle, p: Point) -> bool {
        let (x, y) = (p.x, p.y + self.scroll_y);
        let column = (x / self.cell_w(bounds)).floor() as usize;
        let row = (y / self.cell_h).floor() as usize;
        column >= self.columns || row * self.columns + column >= self.count
    }
}

impl<Theme, R: renderer::Renderer> Widget<Message, Theme, R> for Band {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(&mut self, _: &mut Tree, _: &R, limits: &layout::Limits) -> layout::Node {
        layout::Node::new(limits.resolve(Length::Fill, Length::Fill, Size::ZERO))
    }

    fn update(&mut self, tree: &mut Tree, event: &Event, layout: Layout<'_>, cursor: mouse::Cursor, _: &R, _: &mut dyn Clipboard, shell: &mut Shell<'_, Message>, _: &Rectangle) {
        let state = tree.state.downcast_mut::<State>();
        let bounds = layout.bounds();
        if state.press.is_some() {
            // The pane ended this sweep itself (Escape, a navigation): drop it without a word.
            if state.epoch != self.epoch {
                Band::reset(state);
                state.epoch = self.epoch;
                return;
            }
            if !self.enabled || state.shape != self.shape(bounds) {
                if state.begun {
                    shell.publish(Message::Band(self.side, Ev::Cancel));
                }
                Band::reset(state);
                return;
            }
        }
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(p) = cursor.position_in(bounds) else { return };
                if !self.enabled || !self.on_ground(bounds, p) {
                    return;
                }
                *state = State {
                    press: Some((cursor.position().unwrap_or_default(), Point::new(p.x, p.y + self.scroll_y))),
                    at: cursor.position().unwrap_or_default(),
                    epoch: self.epoch,
                    shape: self.shape(bounds),
                    ..State::default()
                };
                shell.publish(Message::Band(self.side, Ev::Press));
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                let Some((origin, _)) = state.press else { return };
                state.at = *position;
                if !state.begun && origin.distance(*position) < DRAG_DISTANCE {
                    return;
                }
                self.update_cells(state, bounds, shell);
                shell.request_redraw();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.press.is_none() {
                    return;
                }
                if state.begun {
                    self.update_cells(state, bounds, shell);
                    shell.publish(Message::Band(self.side, Ev::End));
                }
                Band::reset(state);
                shell.request_redraw();
            }
            Event::Window(iced::window::Event::RedrawRequested(now)) if state.begun => {
                // A scroll under a still pointer moves the band's far corner over other cells.
                self.update_cells(state, bounds, shell);
                let dir = if state.at.y < bounds.y { -1.0 } else if state.at.y > bounds.y + bounds.height { 1.0 } else { 0.0 };
                if dir == 0.0 {
                    state.frame = None;
                    return;
                }
                if let Some(then) = state.frame {
                    let dt = now.saturating_duration_since(then).as_secs_f32().min(0.1);
                    shell.publish(Message::Band(self.side, Ev::Scroll(dir * SCROLL_PX_PER_S * dt)));
                }
                state.frame = Some(*now);
                shell.request_redraw();
            }
            _ => {}
        }
    }

    fn draw(&self, tree: &Tree, renderer: &mut R, _: &Theme, _: &renderer::Style, layout: Layout<'_>, _: mouse::Cursor, _: &Rectangle) {
        let state = tree.state.downcast_ref::<State>();
        let (Some((_, anchor)), true) = (state.press, state.begun && state.epoch == self.epoch) else { return };
        let bounds = layout.bounds();
        let end = self.end(bounds, state.at);
        let (x, y) = (anchor.x.min(end.x), anchor.y.min(end.y) - self.scroll_y);
        let rect = Rectangle { x: bounds.x + x, y: bounds.y + y, width: (end.x - anchor.x).abs(), height: (end.y - anchor.y).abs() };
        // Clipped to the band, so the part scrolled out of it never draws over the chrome beside it.
        renderer.with_layer(bounds, |r| {
            r.fill_quad(
                renderer::Quad { bounds: rect, border: Border { color: self.accent, width: 1.0, radius: 0.0.into() }, ..Default::default() },
                Color { a: FILL_ALPHA, ..self.accent },
            )
        });
    }
}

impl Flea {
    pub(crate) fn band_message(&mut self, side: u8, ev: Ev) -> Task<Message> {
        if let Ev::Scroll(dy) = ev {
            return operation::scroll_by(list_id(side), scrollable::AbsoluteOffset { x: 0.0, y: dy });
        }
        // SelectionBand.onPressed takes the keyboard into the listing of the pane it was pressed in.
        let focused = if matches!(ev, Ev::Press) { self.focus_side(side) } else { Task::none() };
        let additive = self.modifiers.control();
        let rest = self.with_side(side, |f| {
            let effects = match ev {
                Ev::Press => {
                    f.pane.band_press(additive);
                    vec![]
                }
                Ev::Move { cells, columns, back_x, back_y } => {
                    f.pane.band_update(cells, columns, back_x, back_y);
                    vec![]
                }
                Ev::End => f.pane.band_finish(false),
                Ev::Cancel => f.pane.band_finish(true),
                Ev::Scroll(_) => vec![],
            };
            f.run(effects)
        });
        Task::batch([focused, rest])
    }
}
