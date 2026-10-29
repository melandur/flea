// Stage 5b's half of the preview: a PDF page rendered on the poppler worker and the controls over it,
// and media playing through GStreamer with ui/MediaStrip.qml's play, clock and mute. Both are
// dropped the moment the preview shows anything else, which for media is what stops it playing.
use crate::media::{self, Media};
use crate::pdf;
use crate::{blocking, marks, Flea, Message, PAD_X, ROW_H};
use iced::widget::{button, column, container, image, row, scrollable, space, svg, text, Id};
use iced::{ContentFit, Element, Length, Task};

pub const PDF_ID: &str = "preview-pdf";

#[derive(Debug)]
pub struct PdfView {
    pub path: String,
    pub index: usize,
    pub count: usize,
    pub zoom: f32,
    pub page: Option<image::Handle>,
    pub failed: bool,
}

#[derive(Default)]
pub struct Playback {
    pub media: Option<Media>,
    pub frame: Option<(u64, image::Handle)>,
    pub failed: bool,
    // MediaSound: one session flag, kept across previews and never written to ui.json.
    pub muted: bool,
}

impl Flea {
    // The preview's messages, routed here so the shell's own match stays one line for all of them.
    pub(crate) fn preview_message(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::PreviewLoaded(path, result) => self.preview_loaded(path, result),
            Message::PreviewImage(path, handle) => self.preview_image(path, handle),
            Message::PreviewSettled(generation) => {
                let effects = self.preview.settled(generation);
                self.run_preview(effects)
            }
            Message::PreviewClose => {
                let effects = self.preview.close();
                self.run_preview(effects)
            }
            Message::PdfRendered(result) => self.pdf_rendered(result),
            Message::PdfTurn(delta) => self.pdf_turn(delta),
            Message::PdfZoom(steps) => self.pdf_zoom(steps),
            Message::MediaTick => self.media_tick(),
            Message::MediaToggle => {
                if let Some(m) = self.playback.media.as_mut() {
                    m.toggle_play();
                }
                Task::none()
            }
            Message::MediaMute => {
                self.toggle_mute();
                Task::none()
            }
            Message::TableScrolled(y) => {
                self.table_scroll = y;
                Task::none()
            }
            _ => Task::none(),
        }
    }

    // The page's frame: the card's inner box less the header row and the strip under the page.
    fn pdf_frame(&self) -> (f32, f32) {
        let area_w = self.pane_width();
        let area_h = self.viewport_h + ROW_H;
        let share = if self.preview.expanded { 1.0 } else { crate::preview::INSET };
        ((area_w * share - 2.0 * PAD_X).max(64.0), (area_h * share - 2.0 * PAD_X - 2.0 * ROW_H - 16.0).max(64.0))
    }

    pub(crate) fn clear_media(&mut self) {
        self.pdf = None;
        self.playback.media = None;
        self.playback.frame = None;
        self.playback.failed = false;
    }

    pub(crate) fn load_pdf(&mut self, path: String) -> Task<Message> {
        self.clear_media();
        self.pdf = Some(PdfView { path, index: 0, count: 0, zoom: pdf::MIN_ZOOM, page: None, failed: false });
        self.render_pdf()
    }

    pub(crate) fn render_pdf(&mut self) -> Task<Message> {
        let Some(v) = &self.pdf else { return Task::none() };
        let (path, index, zoom, frame) = (v.path.clone(), v.index, v.zoom, self.pdf_frame());
        Task::perform(blocking(move || pdf::page(&path, index, frame, zoom)), Message::PdfRendered)
    }

    pub(crate) fn pdf_rendered(&mut self, result: Result<pdf::Page, ()>) -> Task<Message> {
        let Some(v) = self.pdf.as_mut() else { return Task::none() };
        match result {
            // A page for a document no longer open, or a page since turned past, draws nothing.
            Ok(p) if p.path == v.path && p.index == v.index.min(p.count.saturating_sub(1)) => {
                v.index = p.index;
                v.count = p.count;
                v.page = Some(image::Handle::from_rgba(p.width, p.height, p.rgba));
            }
            Ok(_) => {}
            Err(()) => v.failed = true,
        }
        Task::none()
    }

    pub(crate) fn pdf_turn(&mut self, delta: i32) -> Task<Message> {
        let Some(v) = self.pdf.as_mut() else { return Task::none() };
        let last = v.count.saturating_sub(1) as i64;
        let next = (v.index as i64 + delta as i64).clamp(0, last) as usize;
        if next == v.index {
            return Task::none();
        }
        v.index = next;
        Task::batch([self.render_pdf(), iced::widget::operation::snap_to(PDF_ID, scrollable::RelativeOffset::START)])
    }

    pub(crate) fn pdf_zoom(&mut self, steps: i32) -> Task<Message> {
        let Some(v) = self.pdf.as_mut() else { return Task::none() };
        v.zoom = pdf::zoom_by(v.zoom, steps);
        self.render_pdf()
    }

    pub(crate) fn open_media(&mut self, path: &str) -> Task<Message> {
        self.clear_media();
        match Media::open(path, self.playback.muted) {
            Ok(m) => self.playback.media = Some(m),
            Err(_) => self.playback.failed = true,
        }
        Task::none()
    }

    pub(crate) fn media_tick(&mut self) -> Task<Message> {
        self.column_media_tick();
        let Some(m) = self.playback.media.as_mut() else { return Task::none() };
        m.poll();
        self.playback.failed |= m.failed;
        let drawn = self.playback.frame.as_ref().map(|(s, _)| *s).unwrap_or(0);
        if let Some(f) = m.frame_after(drawn) {
            self.playback.frame = Some((f.serial, image::Handle::from_rgba(f.width, f.height, f.rgba)));
        }
        Task::none()
    }

    pub(crate) fn toggle_mute(&mut self) {
        self.playback.muted = !self.playback.muted;
        if let Some(m) = &self.playback.media {
            m.set_muted(self.playback.muted);
        }
    }

    fn strip_button(&self, mark: &'static str, msg: Message) -> Element<'_, Message> {
        let p = self.palette;
        button(svg(marks::handle(mark, p.foreground)).width(14).height(14)).on_press(msg).style(move |_, _| button::Style {
            background: None,
            text_color: p.foreground,
            ..Default::default()
        }).into()
    }

    // ui/PdfViewer.qml's surface: the page, scrollable once zoomed past the frame, and its strip.
    pub(crate) fn pdf_view<'a>(&'a self, v: &'a PdfView) -> Element<'a, Message> {
        let p = self.palette;
        let page: Element<'a, Message> = match &v.page {
            Some(h) => scrollable(container(image(h.clone()).content_fit(ContentFit::None)).center_x(Length::Fill))
                .id(Id::new(PDF_ID))
                .direction(scrollable::Direction::Both { vertical: Default::default(), horizontal: Default::default() })
                .height(Length::Fill)
                .into(),
            None => space().height(Length::Fill).into(),
        };
        let pager = text(if v.count > 0 { format!("{} / {}", v.index + 1, v.count) } else { String::new() }).size(11.0).color(p.muted);
        let strip = row![
            self.strip_button("chevron-left", Message::PdfTurn(-1)),
            pager,
            self.strip_button("chevron-right", Message::PdfTurn(1)),
            space().width(Length::Fill),
            self.strip_button("minus", Message::PdfZoom(-1)),
            self.strip_button("plus", Message::PdfZoom(1)),
        ]
        .spacing(6)
        .align_y(iced::Center)
        .height(ROW_H);
        column![page, strip].into()
    }

    // ui/PreviewMedia.qml and ui/MediaStrip.qml: the video's newest frame, or for audio the mark,
    // the name and its facts; under either, play or pause, the clock, and mute.
    pub(crate) fn media_view(&self, video: bool) -> Element<'_, Message> {
        let p = self.palette;
        let surface: Element<'_, Message> = match (&self.playback.frame, video) {
            (Some((_, h)), true) => image(h.clone()).content_fit(ContentFit::Contain).width(Length::Fill).height(Length::Fill).into(),
            (None, true) => space().height(Length::Fill).into(),
            (_, false) => {
                let mut facts = vec![];
                if !self.preview.kind_name.is_empty() {
                    facts.push(self.preview.kind_name.clone());
                }
                let duration = self.playback.media.as_ref().map(Media::duration_ms).unwrap_or(0);
                if duration > 0 {
                    facts.push(media::clock(duration));
                }
                facts.push(crate::format::size(self.preview.size));
                let words = column![text(self.preview.name().to_string()).color(p.foreground), text(facts.join(" · ")).size(11.0).color(p.muted)];
                container(row![svg(marks::handle("music", p.muted)).width(40).height(40), words].spacing(PAD_X).align_y(iced::Center)).center(Length::Fill).into()
            }
        };
        let m = self.playback.media.as_ref();
        let playing = m.is_some_and(|m| m.playing);
        let clock = format!("{} / {}", media::clock(m.map(Media::position_ms).unwrap_or(0)), media::clock(m.map(Media::duration_ms).unwrap_or(0)));
        let strip = row![
            self.strip_button(if playing { "pause" } else { "play" }, Message::MediaToggle),
            text(clock).size(11.0).color(p.muted),
            space().width(Length::Fill),
            self.strip_button(if self.playback.muted { "volume-x" } else { "volume" }, Message::MediaMute),
        ]
        .spacing(8)
        .align_y(iced::Center)
        .height(ROW_H);
        column![surface, strip].into()
    }
}
