// The window's tab strip, ui/TabBar.qml: hidden with no height until a second tab exists, so the
// default window keeps the chrome-to-listing layout. Each tab is its folder's label over the plate,
// the current one on the window's ground with the accent's edge on the strip's bottom; its close
// mark never moves and its target never shrinks; a middle press closes; the + opens a new tab where
// Settings > View > Opening says. tabs.rs is the model; this is the strip and what it asks for.
use crate::tabs::{self, TabMove};
use crate::{marks, Flea, Message, PAD_X, ROW_H};
use iced::widget::{button, container, mouse_area, row, space, stack, svg, text, Row};
use iced::{Color, Element, Length, Task};

// Theme.hitMin, the close target and the + button.
const HIT: f32 = 24.0;
const CAPTION: f32 = 11.0;

#[derive(Debug, Clone)]
pub enum Msg {
    Select(usize),
    Close(usize),
    New,
}

impl Flea {
    pub(crate) fn tabs_open(&self) -> bool {
        self.pane.tab_count() > 1
    }

    // The strip's height, which every surface placed by window height steps down past.
    pub(crate) fn tab_strip_h(&self) -> f32 {
        if self.tabs_open() { ROW_H } else { 0.0 }
    }

    fn current_view(&self) -> String {
        self.dual_from.clone()
    }

    pub(crate) fn tab_message(&mut self, msg: Msg) -> Task<Message> {
        let (view, was) = (self.current_view(), self.tabs_open());
        let m = match msg {
            Msg::Select(i) => self.pane.tab_select(i, &view),
            Msg::Close(i) => self.pane.tab_close(i, &view),
            Msg::New => return self.open_tab_at(String::new()),
        };
        self.run_tab(m, was)
    }

    // Tabs.openNew: an empty target is Settings > View > Opening's, which defaults to this folder.
    pub(crate) fn open_tab_at(&mut self, target: String) -> Task<Message> {
        let target = if !target.is_empty() {
            target
        } else {
            match self.prefs.new_tab.as_str() {
                "home" => self.rail.home.clone(),
                "start" => self.prefs.start_path(&self.rail.home, None),
                _ => String::new(),
            }
        };
        let here = if target.is_empty() { self.pane.tab_path(self.pane.tabs.index) } else { target };
        let (view, was) = (self.current_view(), self.tabs_open());
        let m = self.pane.tab_open(&here, &view);
        self.run_tab(m, was)
    }

    fn run_tab(&mut self, m: TabMove, was_open: bool) -> Task<Message> {
        let TabMove { effects, say, view, moved } = m;
        let mut tasks = vec![];
        // The strip appearing or going takes its row from the listing, or gives it back.
        match (was_open, self.tabs_open()) {
            (false, true) => self.viewport_h = (self.viewport_h - ROW_H).max(ROW_H),
            (true, false) => self.viewport_h += ROW_H,
            _ => {}
        }
        if let Some(text) = say {
            tasks.push(self.say(text));
        }
        if moved && self.preview.active {
            let effects = self.preview.close();
            tasks.push(self.run_preview(effects));
        }
        tasks.push(self.run(effects));
        // A tab restores its own view; the split's panes are list-only, so there it keeps the split's.
        if let Some(v) = view.filter(|v| *v != self.dual_from && self.other.is_none()) {
            let was = (self.grid_mode(), self.columns_mode());
            self.dual_from = v;
            tasks.push(self.after_view_change(was));
        }
        Task::batch(tasks)
    }

    // TabBar's tabWidth: the strip shared out, never narrower than three targets nor wider than a
    // twelve-character label with its close target and padding.
    fn tab_width(&self) -> f32 {
        let n = self.pane.tab_count().max(1) as f32;
        let avail = (self.window_w - HIT).max(0.0);
        let max_w = (CAPTION * 12.0).round() + HIT + 2.0 * PAD_X;
        (avail / n).min(max_w).max(HIT * 3.0).round()
    }

    pub(crate) fn tab_bar(&self) -> Option<Element<'_, Message>> {
        if !self.tabs_open() {
            return None;
        }
        let p = self.palette;
        let (n, current, width) = (self.pane.tab_count(), self.pane.tabs.index, self.tab_width());
        let mut strip = Row::new();
        for i in 0..n {
            let on = i == current;
            let title = tabs::label(&self.pane.tab_path(i), &self.rail.home);
            let close = button(svg(marks::handle("x", if on { p.foreground } else { p.muted })).width(12).height(12))
                .on_press(Message::Tab(Msg::Close(i)))
                .width(HIT)
                .height(ROW_H)
                .padding([(ROW_H - 12.0) / 2.0, (HIT - 12.0) / 2.0])
                .style(|_, _| button::Style { background: None, ..Default::default() });
            let label = text(title).size(CAPTION).color(if on { p.foreground } else { p.muted }).width(Length::Fill).wrapping(text::Wrapping::None);
            let body = row![container(label).padding([0.0, PAD_X]).width(Length::Fill).clip(true), close].align_y(iced::Center);
            let plate = container(body).width(width).height(ROW_H).align_y(iced::Center).style(move |_| {
                if on { container::Style::default().background(p.background) } else { container::Style::default() }
            });
            // The current tab's accent edge replaces the strip's bottom hairline under it.
            let edge = container(space().width(width).height(2)).style(move |_| container::Style::default().background(if on { p.accent } else { Color::TRANSPARENT }));
            let tab = stack![plate, container(edge).height(ROW_H).align_y(iced::Bottom)];
            strip = strip.push(mouse_area(tab).on_press(Message::Tab(Msg::Select(i))).on_middle_press(Message::Tab(Msg::Close(i))));
        }
        let add = button(svg(marks::handle("plus", p.foreground)).width(12).height(12))
            .on_press(Message::Tab(Msg::New))
            .width(HIT)
            .height(ROW_H)
            .padding([(ROW_H - 12.0) / 2.0, (HIT - 12.0) / 2.0])
            .style(|_, _| button::Style { background: None, ..Default::default() });
        let bar = container(strip.push(add)).padding([0.0, PAD_X]).width(Length::Fill).height(ROW_H).style(move |_| container::Style::default().background(p.surface));
        let rule = container(space().height(1)).width(Length::Fill).style(move |_| container::Style::default().background(Color { a: 0.12, ..p.foreground }));
        Some(iced::widget::column![bar, rule].into())
    }
}
