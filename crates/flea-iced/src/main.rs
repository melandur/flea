// Flea's window in iced: the listing pane and the write operations over the unchanged `flea --backend`.
// Everything the QML tree does that is not here yet says so rather than doing nothing.
mod backend;
mod format;
#[cfg(test)]
mod live_tests;
mod ops;
mod pane;
mod prefs;
mod theme;
mod wire;

use backend::Backend;
use iced::keyboard::{self, key::Named, Key};
use iced::widget::{column, container, mouse_area, operation, row, scrollable, space, text, text_input, Id};
use iced::{window, Background, Border, Color, Element, Font, Length, Subscription, Task};
use pane::{Effect, Pane};
use std::sync::Arc;
use std::time::{Duration, Instant};
use theme::Palette;
use wire::{Order, Request};

// ui/Theme.qml's measured box metrics at Omarchy's default text size: a 37 px row, read off the IPC
// seam as Theme.rowHeight, around a bodySmall of 12 px drawn at a 1.8 line box.
const TEXT_PX: f32 = 13.0;
const ROW_H: f32 = 37.0;
const PAD_X: f32 = 14.0;
const SIZE_W: f32 = 90.0;
const DATE_W: f32 = 140.0;
const LIST_ID: &str = "listing";
const RENAME_ID: &str = "rename";
// The second press of a double click lands inside this, iced's own mouse::click interval.
const DOUBLE_TAP: Duration = Duration::from_millis(300);
// Longer than the backend's own 25 s drain limit; see ui/Backend.qml quitDeadline.
const QUIT_DEADLINE: Duration = Duration::from_secs(30);

struct Flea {
    pane: Pane,
    backend: Option<Arc<Backend>>,
    palette: Palette,
    // The status bar's one transient slot: text, generation, and whether it is an error.
    message: Option<(String, u64, bool)>,
    modifiers: keyboard::Modifiers,
    last_tap: Option<(usize, Instant)>,
    message_gen: u64,
    opening: bool,
    quitting: bool,
    viewport_h: f32,
    scroll_y: f32,
}

#[derive(Debug, Clone)]
enum Message {
    Backend(backend::Event),
    Key(keyboard::Event),
    Scrolled(scrollable::Viewport),
    RowPressed(usize),
    RenameInput(String),
    RenameSubmit,
    Opened(Option<i32>, String),
    WatchFired,
    Expired(u64),
    Saved(bool),
    CloseRequested,
    QuitDeadline,
}

fn sleep(d: Duration) -> impl std::future::Future<Output = ()> {
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
fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> impl std::future::Future<Output = T> {
    let (tx, rx) = iced::futures::channel::oneshot::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    async move { rx.await.expect("the worker thread answers before it ends") }
}

fn boot() -> (Flea, Task<Message>) {
    let bin = backend::binary();
    let prefs = prefs::Prefs::load(&bin);
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".into());
    let argv = std::env::args().nth(1);
    let start = prefs.start_path(&home, argv.as_deref());
    let order = Order {
        by: prefs.sort_by.clone(),
        desc: prefs.sort_desc,
        folders_first: prefs.folders_first,
        group_by_kind: prefs.group_by_kind,
    };
    // 36 rows is the tiled height measured on this box; the first viewport report corrects it.
    let pane = Pane::new(prefs.hidden, 36).with_order(order, prefs.wrap_at_ends);
    let mut flea = Flea {
        pane,
        backend: None,
        palette: Palette::load(),
        message: None,
        message_gen: 0,
        modifiers: keyboard::Modifiers::default(),
        last_tap: None,
        opening: false,
        quitting: false,
        viewport_h: 36.0 * ROW_H,
        scroll_y: 0.0,
    };
    let task = match Backend::spawn() {
        Ok(b) => {
            flea.backend = Some(b);
            let effects = flea.pane.open(start, None);
            flea.run(effects)
        }
        Err(e) => {
            flea.pane.error = Some(pane::sentence("backend", &e, ""));
            Task::none()
        }
    };
    (flea, task)
}

impl Flea {
    fn say(&mut self, text: String) -> Task<Message> {
        self.show(text, false)
    }

    fn show(&mut self, text: String, alert: bool) -> Task<Message> {
        self.message_gen += 1;
        let gen = self.message_gen;
        self.message = Some((text, gen, alert));
        Task::perform(sleep(Duration::from_millis(pane::MESSAGE_MS)), move |_| Message::Expired(gen))
    }

    fn run(&mut self, effects: Vec<Effect>) -> Task<Message> {
        let mut tasks = vec![];
        for effect in effects {
            match effect {
                Effect::Send(request) => {
                    let sent = self.backend.as_ref().map(|b| b.send(&request));
                    if let Some(Err(e)) = sent {
                        tasks.push(self.say(pane::sentence("backend", &e, "")));
                    }
                }
                Effect::ScrollTo(index) => {
                    let y = index as f32 * ROW_H;
                    tasks.push(operation::scroll_to(LIST_ID, scrollable::AbsoluteOffset { x: 0.0, y }));
                }
                Effect::OpenFile(path) => tasks.push(self.open_file(path)),
                Effect::WatchTimer => tasks.push(Task::perform(
                    sleep(Duration::from_millis(pane::WATCH_MS)),
                    |_| Message::WatchFired,
                )),
                Effect::Message(text) => tasks.push(self.show(text, false)),
                Effect::Alert(text) => tasks.push(self.show(text, true)),
                Effect::FocusRename => tasks.push(operation::focus(RENAME_ID)),
                Effect::SaveHidden(hidden) => {
                    let bin = backend::binary();
                    tasks.push(Task::perform(blocking(move || prefs::save_hidden(&bin, hidden)), Message::Saved));
                }
            }
        }
        Task::batch(tasks)
    }

    // ui/Opener.qml: one `flea --open` at a time, and a second press while it runs says so.
    fn open_file(&mut self, path: String) -> Task<Message> {
        if self.opening {
            return self.say("Still opening the last file; try again in a moment.".into());
        }
        self.opening = true;
        let bin = backend::binary();
        Task::perform(
            blocking(move || {
                let code = std::process::Command::new(&bin).arg("--open").arg(&path).status().ok().and_then(|s| s.code());
                (code, path)
            }),
            |(code, path)| Message::Opened(code, path),
        )
    }

    fn quit(&mut self) -> Task<Message> {
        if self.quitting {
            return Task::none();
        }
        self.quitting = true;
        // The backend drains as it quits, so a copy in flight removes its own partial destination.
        match self.backend.as_ref().map(|b| b.send(&Request::Quit)) {
            Some(Ok(())) => Task::perform(sleep(QUIT_DEADLINE), |_| Message::QuitDeadline),
            _ => iced::exit(),
        }
    }

    fn key(&mut self, event: keyboard::Event) -> Task<Message> {
        if let keyboard::Event::ModifiersChanged(m) = event {
            self.modifiers = m;
            return Task::none();
        }
        let keyboard::Event::KeyPressed { key, modifiers, repeat, .. } = event else { return Task::none() };
        if key == Key::Named(Named::Escape) {
            // An error is dismissed before anything else Escape unwinds.
            if self.message.as_ref().is_some_and(|m| m.2) {
                self.message = None;
                return Task::none();
            }
            let effects = self.pane.escape();
            return self.run(effects);
        }
        // The editor owns every other key while it is open; Enter reaches it through on_submit.
        if self.pane.renaming.is_some() {
            return Task::none();
        }
        let effects = match (key.as_ref(), modifiers.control()) {
            (Key::Named(Named::ArrowDown), false) => self.pane.step(1),
            (Key::Named(Named::ArrowUp), false) => self.pane.step(-1),
            (Key::Named(Named::ArrowLeft), false) => self.pane.up(),
            (Key::Named(Named::ArrowRight), false) => self.pane.forward(),
            (Key::Named(Named::Enter), false) => self.pane.activate(),
            (Key::Named(Named::F5), false) => self.pane.reread(),
            (Key::Character("h"), true) => self.pane.toggle_hidden(),
            (Key::Character("c"), true) => self.pane.copy(false),
            (Key::Character("x"), true) => self.pane.copy(true),
            (Key::Character("v"), true) => self.pane.paste(),
            (Key::Character("z"), true) => self.pane.undo(),
            (Key::Character("a"), true) => self.pane.select_all(),
            (Key::Character("n"), true) => self.pane.new_folder(),
            (Key::Named(Named::Delete), false) => self.pane.trash(),
            (Key::Named(Named::F2), false) => self.pane.start_rename(),
            // A held Space toggles once; the paint gesture of ui/js/Marks.js is not ported yet.
            (Key::Named(Named::Space), false) if !repeat => self.pane.toggle_select(),
            (Key::Character(c), true) if "fbgtmjkl".contains(c) => {
                return self.say(format!("Ctrl+{} is not in this build yet.", c.to_uppercase()));
            }
            (Key::Named(Named::Tab), false) => return self.say("That key is not in this build yet.".into()),
            _ => return Task::none(),
        };
        self.run(effects)
    }

    // Every input ends by asking whether the watched re-read it was holding can now be paid.
    fn update(&mut self, message: Message) -> Task<Message> {
        let task = self.handle(message);
        let owed = self.pane.pay_debt();
        Task::batch([task, self.run(owed)])
    }

    // ui/js/Tap.js reads a tap count, and iced's press arrives on both clicks of a double, so the
    // count is taken here: a second press on the same row inside the interval is the second tap.
    fn tap(&mut self, index: usize) -> Task<Message> {
        let now = Instant::now();
        let taps = match self.last_tap {
            Some((i, at)) if i == index && now.duration_since(at) < DOUBLE_TAP => 2,
            _ => 1,
        };
        self.last_tap = if taps == 1 { Some((index, now)) } else { None };
        let effects = self.pane.tap(index, taps, self.modifiers.control(), self.modifiers.shift());
        self.run(effects)
    }

    fn handle(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Backend(backend::Event::Reply(reply)) => {
                let effects = self.pane.receive(reply);
                self.run(effects)
            }
            Message::Backend(backend::Event::Unreadable) => {
                self.say("The backend sent a line this build cannot read.".into())
            }
            Message::Backend(backend::Event::Exited(code)) => {
                self.backend = None;
                if self.quitting {
                    return iced::exit();
                }
                let code = code.map(|c| c.to_string()).unwrap_or_else(|| "a signal".into());
                self.say(format!("The backend exited with code {code}."))
            }
            Message::Key(event) => self.key(event),
            Message::Scrolled(viewport) => {
                self.scroll_y = viewport.absolute_offset().y;
                self.viewport_h = viewport.bounds().height;
                let first = (self.scroll_y / ROW_H).floor().max(0.0) as usize;
                // Fully visible rows, which is what the cursor clamp wants; the view draws one more.
                let visible = (self.viewport_h / ROW_H).floor().max(1.0) as usize;
                let effects = self.pane.viewport(first, visible);
                self.run(effects)
            }
            Message::RowPressed(i) => self.tap(i),
            Message::RenameInput(draft) => {
                self.pane.rename_input(draft);
                Task::none()
            }
            Message::RenameSubmit => {
                let effects = self.pane.commit_rename();
                self.run(effects)
            }
            Message::Opened(code, path) => {
                self.opening = false;
                match code {
                    Some(0) => Task::none(),
                    // 3 is a directory the opener refuses to hand on: navigate instead.
                    Some(3) => {
                        let effects = self.pane.open(path, None);
                        self.run(effects)
                    }
                    _ => self.say("That file could not be opened.".into()),
                }
            }
            Message::WatchFired => {
                let effects = self.pane.watch_fired();
                self.run(effects)
            }
            Message::Expired(gen) => {
                if self.message.as_ref().is_some_and(|m| m.1 == gen) {
                    self.message = None;
                }
                Task::none()
            }
            Message::Saved(ok) => {
                if ok {
                    Task::none()
                } else {
                    self.say("That setting could not be saved.".into())
                }
            }
            Message::CloseRequested => self.quit(),
            Message::QuitDeadline => iced::exit(),
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let p = self.palette;
        let header = row![
            text("Name").width(Length::Fill),
            text("Size").width(SIZE_W),
            text("Date Modified").width(DATE_W),
        ]
        .padding([0.0, PAD_X])
        .height(ROW_H)
        .align_y(iced::Center);
        let header = container(header.spacing(8))
            .style(move |_| container::Style::default().color(p.muted))
            .width(Length::Fill);

        let body: Element<'_, Message> = if let Some(err) = &self.pane.error {
            container(text(err.clone()).color(p.muted)).center(Length::Fill).into()
        } else if self.pane.listed_once && self.pane.total == 0 {
            container(text("This folder is empty.").color(p.muted)).center(Length::Fill).into()
        } else {
            self.listing()
        };

        let mut count = if self.pane.total == 1 { "1 item".to_string() } else { format!("{} items", format::count(self.pane.total)) };
        if !self.pane.selection.is_empty() {
            count += &format!(" · {} selected", format::count(self.pane.selection.len()));
        }
        // A transient message wins the slot; the running transfer's sticky line is under it.
        let (note, ink) = match &self.message {
            Some((t, _, alert)) => (t.clone(), if *alert { p.error } else { p.foreground }),
            None => (self.pane.sticky.clone(), p.foreground),
        };
        let status = row![
            text(self.pane.path.clone()).width(Length::Fill).wrapping(text::Wrapping::None),
            text(note).color(ink),
            text(count),
        ]
        .spacing(16)
        .padding([0.0, PAD_X])
        .height(ROW_H)
        .align_y(iced::Center);
        let status = container(status).style(move |_| container::Style::default().color(p.muted).background(p.surface));

        container(column![header, body, status])
            .style(move |_| container::Style::default().background(p.background).color(p.foreground))
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    // Rule 2: a spacer above, the rows on screen, a spacer below. Nothing else is ever built.
    fn listing(&self) -> Element<'_, Message> {
        let total = self.pane.total;
        let first = ((self.scroll_y / ROW_H).floor().max(0.0) as usize).min(total);
        let shown = ((self.viewport_h / ROW_H).ceil() as usize + 1).min(total - first);
        let mut rows = column![space().height(first as f32 * ROW_H)];
        for i in first..first + shown {
            rows = rows.push(self.row(i));
        }
        rows = rows.push(space().height((total - first - shown) as f32 * ROW_H));
        scrollable(rows.width(Length::Fill))
            .id(Id::new(LIST_ID))
            .on_scroll(Message::Scrolled)
            .height(Length::Fill)
            .width(Length::Fill)
            .into()
    }

    fn row(&self, i: usize) -> Element<'_, Message> {
        let p = self.palette;
        let cursor = i == self.pane.cursor;
        let selected = self.pane.selection.contains(&i);
        let editing = self.pane.renaming.as_ref().filter(|r| r.index == i);
        let cells: Element<'_, Message> = match self.pane.row_at(i) {
            Some(_) if editing.is_some() => {
                let r = editing.unwrap();
                let field = text_input("Name", &r.draft)
                    .id(Id::new(RENAME_ID))
                    .on_input_maybe(r.pending.is_none().then_some(Message::RenameInput))
                    .on_submit(Message::RenameSubmit)
                    .size(TEXT_PX)
                    .width(Length::Fill);
                row![field, text(r.error.clone()).color(p.error)].spacing(12).align_y(iced::Center).into()
            }
            Some(r) => {
                let link = format::is_symlink(r.p);
                let ink = if link {
                    p.symlink
                } else if r.d {
                    p.foreground
                } else if format::is_executable(r.p) {
                    p.executable
                } else {
                    p.foreground
                };
                let name = match &r.l {
                    Some(target) => format!("{} -> {}", r.n, target),
                    None => r.n.clone(),
                };
                // p 0 is a failed stat, and it condemns s and m with it; see docs/protocol.md "rows".
                let size = if r.d { String::new() } else if link { "link".into() } else if r.p == 0 { "--".into() } else { format::size(r.s) };
                let date = if r.p == 0 { "--".into() } else { format::date(r.m) };
                row![
                    text(name).color(ink).width(Length::Fill).wrapping(text::Wrapping::None),
                    text(size).color(p.muted).width(SIZE_W),
                    text(date).color(p.muted).width(DATE_W),
                ]
                .spacing(8)
                .into()
            }
            // Outside the held window: the row's slot stands until its window arrives.
            None => row![text("").width(Length::Fill)].into(),
        };
        let band = container(cells)
            .padding([0.0, PAD_X])
            .height(ROW_H)
            .width(Length::Fill)
            .align_y(iced::Center)
            .style(move |_| {
                // The cursor draws the accent fill, a marked row that is not the cursor the plain one.
                if cursor {
                    container::Style::default()
                        .background(Background::Color(Color { a: 0.18, ..p.accent }))
                        .border(Border { color: p.accent, width: 0.0, radius: 0.0.into() })
                } else if selected {
                    container::Style::default().background(p.selection)
                } else {
                    container::Style::default()
                }
            });
        mouse_area(band).on_press(Message::RowPressed(i)).into()
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            Subscription::run(backend::events).map(Message::Backend),
            keyboard::listen().map(Message::Key),
            window::close_requests().map(|_| Message::CloseRequested),
        ])
    }
}

fn main() -> iced::Result {
    iced::application(boot, Flea::update, Flea::view)
        .title("Flea")
        .subscription(Flea::subscription)
        .exit_on_close_request(false)
        .default_font(Font::DEFAULT)
        .settings(iced::Settings { default_text_size: TEXT_PX.into(), ..Default::default() })
        // Its own app id while both front ends exist, so a compositor rule or a test can tell them apart.
        .window(window::Settings {
            platform_specific: window::settings::PlatformSpecific {
                application_id: "com.thisisgm.flea.iced".into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .run()
}
