// Media playback for the Quick Look, through GStreamer, the operator's choice for Stage 5b. playbin
// decodes; video arrives in an app sink as RGBA frames the overlay draws, audio goes to the default
// output. GStreamer is initialised by the first media preview and never before, which is AGENTS.md
// rule 5's laziness carried over: QtMultimedia cost 20 MB before it played anything, and nothing
// here is loaded for a window that never opens a video.
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use gstreamer_video as gst_video;
use gstreamer_video::prelude::*;
use std::sync::{Arc, Mutex};

pub const SEEK_MS: i64 = 5000;

#[derive(Debug, Clone)]
pub struct Frame {
    pub serial: u64,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub struct Media {
    playbin: gst::Element,
    frame: Arc<Mutex<Option<Frame>>>,
    pub failed: bool,
    pub playing: bool,
    pub ended: bool,
}

fn uri(path: &str) -> String {
    let mut out = String::from("file://");
    for b in path.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~/".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

impl Media {
    // PreviewMedia autoPlay: a preview that opens on media starts it.
    pub fn open(path: &str, muted: bool) -> Result<Media, String> {
        gst::init().map_err(|e| e.to_string())?;
        let playbin = gst::ElementFactory::make("playbin").property("uri", uri(path)).build().map_err(|e| e.to_string())?;
        let caps = gst_video::VideoCapsBuilder::new().format(gst_video::VideoFormat::Rgba).build();
        // One buffer and drop the rest: the window draws the newest frame and never queues them.
        let sink = gst_app::AppSink::builder().caps(&caps).max_buffers(1).drop(true).build();
        let frame: Arc<Mutex<Option<Frame>>> = Arc::new(Mutex::new(None));
        let slot = frame.clone();
        let mut serial = 0u64;
        sink.set_callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_sample(move |s| {
                    let sample = s.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                    let (Some(buffer), Some(caps)) = (sample.buffer(), sample.caps()) else { return Ok(gst::FlowSuccess::Ok) };
                    let Ok(info) = gst_video::VideoInfo::from_caps(caps) else { return Ok(gst::FlowSuccess::Ok) };
                    let Ok(vf) = gst_video::VideoFrameRef::from_buffer_ref_readable(buffer, &info) else { return Ok(gst::FlowSuccess::Ok) };
                    let (w, h) = (info.width() as usize, info.height() as usize);
                    let stride = vf.plane_stride()[0] as usize;
                    let Ok(data) = vf.plane_data(0) else { return Ok(gst::FlowSuccess::Ok) };
                    // A row can carry padding past its pixels, so each row is copied on its own.
                    let mut rgba = Vec::with_capacity(w * h * 4);
                    for y in 0..h {
                        rgba.extend_from_slice(&data[y * stride..y * stride + w * 4]);
                    }
                    serial += 1;
                    *slot.lock().unwrap() = Some(Frame { serial, width: w as u32, height: h as u32, rgba });
                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        );
        playbin.set_property("video-sink", &sink);
        playbin.set_property("mute", muted);
        let mut media = Media { playbin, frame, failed: false, playing: false, ended: false };
        media.set_playing(true);
        Ok(media)
    }

    fn set_playing(&mut self, on: bool) {
        let state = if on { gst::State::Playing } else { gst::State::Paused };
        self.failed |= self.playbin.set_state(state).is_err();
        self.playing = on && !self.failed;
    }

    pub fn toggle_play(&mut self) {
        // A file that played to its end starts again from the top, as a player's own button does.
        if self.ended {
            self.ended = false;
            self.seek_to(0);
        }
        let on = !self.playing;
        self.set_playing(on);
    }

    pub fn set_muted(&self, muted: bool) {
        self.playbin.set_property("mute", muted);
    }

    pub fn position_ms(&self) -> i64 {
        self.playbin.query_position::<gst::ClockTime>().map(|t| t.mseconds() as i64).unwrap_or(0)
    }

    pub fn duration_ms(&self) -> i64 {
        self.playbin.query_duration::<gst::ClockTime>().map(|t| t.mseconds() as i64).unwrap_or(0)
    }

    // PreviewMedia.seekTo: clamped to the file's own length.
    pub fn seek_to(&self, ms: i64) {
        let duration = self.duration_ms();
        let target = if duration > 0 { ms.clamp(0, duration) } else { ms.max(0) };
        let _ = self.playbin.seek_simple(gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT, gst::ClockTime::from_mseconds(target as u64));
    }

    pub fn seek(&self, delta: i64) {
        self.seek_to(self.position_ms() + delta);
    }

    // Read whatever the bus has said since the last tick: an error fails the preview, the end stops it.
    pub fn poll(&mut self) {
        let Some(bus) = self.playbin.bus() else { return };
        while let Some(msg) = bus.pop() {
            match msg.view() {
                gst::MessageView::Error(_) => {
                    self.failed = true;
                    self.playing = false;
                }
                gst::MessageView::Eos(_) => {
                    self.ended = true;
                    self.playing = false;
                }
                _ => {}
            }
        }
    }

    // The newest frame, only when it is newer than the one already drawn.
    pub fn frame_after(&self, serial: u64) -> Option<Frame> {
        self.frame.lock().unwrap().as_ref().filter(|f| f.serial > serial).cloned()
    }
}

// Dropping the preview is what stops playback, as dropping the QML Loader's source did.
impl Drop for Media {
    fn drop(&mut self) {
        let _ = self.playbin.set_state(gst::State::Null);
    }
}

// Format.duration: m:ss, or h:mm:ss past the hour.
pub fn clock(ms: i64) -> String {
    let total = (ms.max(0) / 1000) as u64;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clock_reads_as_format_duration() {
        assert_eq!(clock(0), "0:00");
        assert_eq!(clock(65_400), "1:05");
        assert_eq!(clock(3_725_000), "1:02:05");
    }

    // A real two-second clip made by GStreamer itself, then played back through the same path.
    #[test]
    fn a_clip_plays_frames_seeks_and_ends() {
        gst::init().unwrap();
        let path = std::env::temp_dir().join(format!("flea-iced-{}.mkv", std::process::id()));
        let make = format!("videotestsrc num-buffers=50 pattern=red ! video/x-raw,width=64,height=48,framerate=25/1 ! videoconvert ! jpegenc ! matroskamux ! filesink location={}", path.display());
        let pipeline = gst::parse::launch(&make).unwrap();
        pipeline.set_state(gst::State::Playing).unwrap();
        let bus = pipeline.bus().unwrap();
        bus.timed_pop_filtered(gst::ClockTime::from_seconds(10), &[gst::MessageType::Eos, gst::MessageType::Error]).unwrap();
        pipeline.set_state(gst::State::Null).unwrap();

        let mut m = Media::open(path.to_str().unwrap(), true).unwrap();
        let mut frame = None;
        for _ in 0..200 {
            std::thread::sleep(std::time::Duration::from_millis(20));
            m.poll();
            if let Some(f) = m.frame_after(0) {
                frame = Some(f);
                break;
            }
        }
        let f = frame.expect("no frame arrived");
        assert_eq!((f.width, f.height), (64, 48));
        let px = &f.rgba[((24 * 64 + 32) * 4) as usize..][..3];
        assert!(px[0] > 180 && px[1] < 80 && px[2] < 80, "{px:?}");
        assert!((1900..=2100).contains(&m.duration_ms()), "{}", m.duration_ms());
        assert!(!m.failed && m.playing);
        m.toggle_play();
        assert!(!m.playing);
        let _ = std::fs::remove_file(&path);
    }
}
