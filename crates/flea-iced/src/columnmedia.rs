// The preview column's two readers that own a resource, ui/PreviewColumn.qml's pdfLoader and
// playerLoader: a PDF's page on the poppler worker with its toolbar under the frame, and media with
// ui/MediaStrip.qml's play, hairline seek, clock and mute. The page is the column's own, not the
// overlay's; the player is built by the first press of play and never before, so browsing a folder
// of clips plays nothing and builds nothing, and it is dropped the moment the column stops showing
// that file, the overlay opens over it, or the strip goes. One file plays in one place.
use crate::columnsview::Msg;
use crate::facts::State;
use crate::media::{self, Media};
use crate::pdf;
use crate::{blocking, marks, Flea, Message, PAD_X, ROW_H};
use iced::widget::{button, container, image, row, scrollable, slider, space, svg, text};
use iced::{Background, Border, Color, ContentFit, Element, Length, Task};

#[derive(Debug)]
pub struct ColPdf {
    pub path: String,
    pub index: usize,
    pub count: usize,
    pub zoom: f32,
    pub page: Option<(image::Handle, u32, u32)>,
    pub failed: bool,
    // The frame the last render was asked for, so a resized column renders again.
    asked: (f32, f32),
}

#[derive(Default)]
pub struct ColumnPlay {
    pub pdf: Option<ColPdf>,
    // The player and the file it was built for.
    pub media: Option<(String, Media)>,
    pub frame: Option<(u64, image::Handle)>,
    // A drag on the seek in flight draws from the pointer, not from the player.
    pub scrub: Option<f32>,
}

// The column's frame box: its third of the pane less the padding, 16 by 10, less the hairline.
pub fn frame_box(pane_width: f32) -> (f32, f32) {
    let width = pane_width - 2.0 * (pane_width / 3.0).floor();
    let inner = width - 2.0 * PAD_X;
    ((inner - 2.0).max(16.0), ((inner * 10.0 / 16.0).round() - 2.0).max(10.0))
}

impl Flea {
    // Run after every input: the page for a PDF row, and the player dropped wherever it may not live.
    pub(crate) fn column_play_follow(&mut self, shown: bool) -> Task<Message> {
        let loaded = self.colprev.model.loaded.as_ref().filter(|_| shown).map(|l| l.path.clone());
        let state = self.colprev.model.row_state();
        let frame = frame_box(self.pane_width());
        let play = &mut self.colprev.play;
        let allowed = loaded.is_some() && matches!(state, Some(State::Video | State::Audio)) && !self.preview.active;
        if !allowed || play.media.as_ref().map(|(p, _)| p) != loaded.as_ref() {
            play.media = None;
            play.frame = None;
            play.scrub = None;
        }
        let Some(path) = loaded.filter(|_| state == Some(State::Pdf)) else {
            play.pdf = None;
            return Task::none();
        };
        match &play.pdf {
            Some(v) if v.path == path && (v.asked == frame || v.failed) => Task::none(),
            Some(v) if v.path == path => self.render_column_pdf(),
            _ => {
                self.colprev.play.pdf = Some(ColPdf { path, index: 0, count: 0, zoom: pdf::MIN_ZOOM, page: None, failed: false, asked: frame });
                self.render_column_pdf()
            }
        }
    }

    fn render_column_pdf(&mut self) -> Task<Message> {
        let frame = frame_box(self.pane_width());
        let Some(v) = self.colprev.play.pdf.as_mut() else { return Task::none() };
        v.asked = frame;
        let (path, index, zoom) = (v.path.clone(), v.index, v.zoom);
        Task::perform(blocking(move || pdf::page(&path, index, frame, zoom)), |r| Message::Column(Msg::PdfRendered(r)))
    }

    pub(crate) fn column_media_message(&mut self, msg: Msg) -> Task<Message> {
        match msg {
            Msg::PdfRendered(result) => {
                let Some(v) = self.colprev.play.pdf.as_mut() else { return Task::none() };
                match result {
                    // A page for a document no longer shown, or one since turned past, draws nothing.
                    Ok(p) if p.path == v.path && p.index == v.index.min(p.count.saturating_sub(1)) => {
                        v.index = p.index;
                        v.count = p.count;
                        v.page = Some((image::Handle::from_rgba(p.width, p.height, p.rgba), p.width, p.height));
                    }
                    Ok(_) => {}
                    Err(()) => v.failed = true,
                }
                Task::none()
            }
            Msg::PdfTurn(delta) => {
                let Some(v) = self.colprev.play.pdf.as_mut() else { return Task::none() };
                let next = (v.index as i64 + delta as i64).clamp(0, v.count.saturating_sub(1) as i64) as usize;
                if next == v.index {
                    return Task::none();
                }
                v.index = next;
                self.render_column_pdf()
            }
            Msg::PdfZoom(steps) => {
                let Some(v) = self.colprev.play.pdf.as_mut() else { return Task::none() };
                v.zoom = pdf::zoom_by(v.zoom, steps);
                self.render_column_pdf()
            }
            Msg::PdfExpand => self.expand_column_pdf(),
            Msg::Play => {
                let Some(l) = self.colprev.model.loaded.as_ref() else { return Task::none() };
                match self.colprev.play.media.as_mut() {
                    Some((_, m)) => m.toggle_play(),
                    // It exists because play was pressed, so it starts.
                    None => {
                        if let Ok(m) = Media::open(&l.path, self.playback.muted) {
                            self.colprev.play.media = Some((l.path.clone(), m));
                        }
                    }
                }
                Task::none()
            }
            Msg::Scrub(fraction) => {
                self.colprev.play.scrub = Some(fraction);
                Task::none()
            }
            // Seeks once, on release: seeking on every dragged frame would fight the player's advance.
            Msg::ScrubDone => {
                let fraction = self.colprev.play.scrub.take();
                let duration = self.column_duration();
                if let (Some(f), Some((_, m))) = (fraction, self.colprev.play.media.as_ref()) {
                    if duration > 0 {
                        m.seek_to((duration as f32 * f) as i64);
                    }
                }
                Task::none()
            }
            Msg::Mute => {
                // MediaSound: one session flag, which the overlay's strip reads too.
                self.toggle_mute();
                if let Some((_, m)) = &self.colprev.play.media {
                    m.set_muted(self.playback.muted);
                }
                Task::none()
            }
            _ => Task::none(),
        }
    }

    // SelectionPreview.onExpandRequested: the overlay opens on this file at the column's page and
    // zoom, filling the window, and the column's own reader stands down under it.
    fn expand_column_pdf(&mut self) -> Task<Message> {
        let Some(l) = self.colprev.model.loaded.clone() else { return Task::none() };
        let (index, zoom) = self.colprev.play.pdf.as_ref().map_or((0, pdf::MIN_ZOOM), |v| (v.index, v.zoom));
        let effects = self.preview.open(l.path.clone(), l.row.i.clone(), l.row.s, l.kind_name.clone(), l.index);
        let opened = self.run_preview(effects);
        self.preview.expanded = true;
        let Some(v) = self.pdf.as_mut().filter(|v| v.path == l.path) else { return opened };
        v.index = index;
        v.zoom = zoom;
        Task::batch([opened, self.render_pdf()])
    }

    pub(crate) fn column_media_tick(&mut self) {
        let play = &mut self.colprev.play;
        let Some((_, m)) = play.media.as_mut() else { return };
        m.poll();
        let drawn = play.frame.as_ref().map_or(0, |(s, _)| *s);
        if let Some(f) = m.frame_after(drawn) {
            play.frame = Some((f.serial, image::Handle::from_rgba(f.width, f.height, f.rgba)));
        }
    }

    pub(crate) fn column_media_playing(&self) -> Option<bool> {
        self.colprev.play.media.as_ref().map(|(_, m)| m.playing)
    }

    // The player's own length once it is playing, the probe's until then.
    fn column_duration(&self) -> i64 {
        let probe = self.colprev.model.meta.as_ref().map_or(0, |m| m.ms as i64);
        match &self.colprev.play.media {
            Some((_, m)) if m.position_ms() > 0 && m.duration_ms() > 0 => m.duration_ms(),
            _ => probe,
        }
    }

    // The page inside the frame: centred at zoom 1, scrollable both ways once zoomed past it. A
    // scrollable gives its content no size to fill, so the box is the larger of frame and page.
    pub(crate) fn column_pdf_layer<'a>(&'a self) -> Option<Element<'a, Message>> {
        let (h, w, ht) = self.colprev.play.pdf.as_ref()?.page.clone()?;
        let (fw, fh) = frame_box(self.pane_width());
        Some(
            scrollable(container(image(h).content_fit(ContentFit::None)).center_x(fw.max(w as f32)).center_y(fh.max(ht as f32)))
                .direction(scrollable::Direction::Both { vertical: Default::default(), horizontal: Default::default() })
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
        )
    }

    pub(crate) fn column_video_layer<'a>(&'a self) -> Option<Element<'a, Message>> {
        let (_, h) = self.colprev.play.frame.as_ref()?;
        Some(container(image(h.clone()).content_fit(ContentFit::Contain).width(Length::Fill).height(Length::Fill)).padding(1).into())
    }

    fn column_button(&self, mark: &'static str, msg: Option<Msg>) -> Element<'_, Message> {
        let p = self.palette;
        let ink = if msg.is_some() { p.foreground } else { Color { a: 0.4, ..p.muted } };
        button(svg(marks::handle(mark, ink)).width(14).height(14))
            .on_press_maybe(msg.map(Message::Column))
            .padding(6)
            .style(move |_, _| button::Style { background: None, text_color: ink, ..Default::default() })
            .into()
    }

    // The PDF's controls stay outside the page frame: the pager, the zoom pair and the maximize mark.
    pub(crate) fn column_pdf_toolbar(&self) -> Option<Element<'_, Message>> {
        let v = self.colprev.play.pdf.as_ref().filter(|v| v.count > 0)?;
        let label = text(format!("{} / {}", v.index + 1, v.count)).size(11.0).color(self.palette.muted);
        Some(
            container(
                row![
                    self.column_button("chevron-left", (v.index > 0).then_some(Msg::PdfTurn(-1))),
                    label,
                    self.column_button("chevron-right", (v.index + 1 < v.count).then_some(Msg::PdfTurn(1))),
                    self.column_button("minus", (v.zoom > pdf::MIN_ZOOM).then_some(Msg::PdfZoom(-1))),
                    self.column_button("plus", (v.zoom < pdf::MAX_ZOOM).then_some(Msg::PdfZoom(1))),
                    self.column_button("maximize", Some(Msg::PdfExpand)),
                ]
                .spacing(8)
                .align_y(iced::Center),
            )
            .center_x(Length::Fill)
            .into(),
        )
    }

    // MediaStrip: play or pause, a hairline seek with a square accent handle, the clock and mute,
    // framed on four sides because it is a tile here and not the overlay's bottom edge.
    pub(crate) fn column_media_strip(&self) -> Element<'_, Message> {
        let p = self.palette;
        let m = self.colprev.play.media.as_ref().map(|(_, m)| m);
        let playing = m.is_some_and(|m| m.playing);
        let position = m.map_or(0, Media::position_ms);
        let duration = self.column_duration();
        let fraction = if duration > 0 { (position as f32 / duration as f32).clamp(0.0, 1.0) } else { 0.0 };
        let shown = self.colprev.play.scrub.unwrap_or(fraction);
        let seek = slider(0.0..=1.0, shown, |f| Message::Column(Msg::Scrub(f)))
            .step(0.001_f32)
            .height(6.0)
            .on_release(Message::Column(Msg::ScrubDone))
            .style(move |_, _| slider::Style {
                rail: slider::Rail { backgrounds: (Background::Color(p.accent), Background::Color(p.muted)), width: 2.0, border: Border::default() },
                handle: slider::Handle {
                    shape: slider::HandleShape::Rectangle { width: 6, border_radius: 0.0.into() },
                    background: Background::Color(p.accent),
                    border_width: 0.0,
                    border_color: Color::TRANSPARENT,
                },
            });
        let clock = text(format!("{} / {}", media::clock(position), media::clock(duration))).size(11.0).color(p.muted);
        let strip = row![
            self.column_button(if playing { "pause" } else { "play" }, Some(Msg::Play)),
            container(seek).width(Length::Fill).center_y(Length::Fill),
            clock,
            self.column_button(if self.playback.muted { "volume-x" } else { "volume" }, Some(Msg::Mute)),
            space().width(2),
        ]
        .spacing(8)
        .align_y(iced::Center);
        container(strip)
            .height(ROW_H)
            .width(Length::Fill)
            .style(move |_| container::Style::default().background(p.surface).border(Border { color: p.muted, width: 1.0, radius: 0.0.into() }))
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_frame_is_the_last_third_less_its_padding_at_sixteen_by_ten() {
        // 1408 px of pane: thirds of 469, the last 470; less 14 a side is 442, so 440 by 274 inside the hairline.
        assert_eq!(frame_box(1408.0), (440.0, 274.0));
        assert_eq!(frame_box(30.0), (16.0, 10.0));
    }
}
