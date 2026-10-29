// A PDF page rendered by poppler-glib into a cairo surface, the operator's choice for Stage 5b. One
// worker thread holds the open document, because poppler's objects stay on the thread that made them
// and parsing the file again for every page turn would cost what QtPdf's PdfDocument never did.
// ui/PdfViewer.qml's contract: one page at a time, zoom 1 fits the whole page to the frame, and the
// ladder runs to 4 in quarter steps.
use std::sync::mpsc::{self, Sender};
use std::sync::OnceLock;

pub const MIN_ZOOM: f32 = 1.0;
pub const MAX_ZOOM: f32 = 4.0;
pub const ZOOM_STEP: f32 = 0.25;

#[derive(Debug, Clone)]
pub struct Page {
    pub path: String,
    pub index: usize,
    pub count: usize,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

struct Job {
    path: String,
    index: usize,
    frame: (f32, f32),
    zoom: f32,
    reply: Sender<Result<Page, ()>>,
}

fn worker() -> &'static Sender<Job> {
    static TX: OnceLock<Sender<Job>> = OnceLock::new();
    TX.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Job>();
        std::thread::spawn(move || {
            let mut open: Option<(String, poppler::Document)> = None;
            for job in rx {
                if open.as_ref().map(|(p, _)| p != &job.path).unwrap_or(true) {
                    open = document(&job.path).map(|d| (job.path.clone(), d));
                }
                let page = open.as_ref().ok_or(()).and_then(|(_, d)| render(d, &job));
                let _ = job.reply.send(page);
            }
        });
        tx
    })
}

fn document(path: &str) -> Option<poppler::Document> {
    let file = gio_uri(path);
    poppler::Document::from_file(&file, None).ok()
}

// poppler takes a file:// URI; every byte outside the unreserved set is escaped.
fn gio_uri(path: &str) -> String {
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

// The page fitted to the frame and multiplied by the zoom, painted on white because a PDF page with
// no background of its own is paper, and the result converted from cairo's native-endian ARGB.
fn render(doc: &poppler::Document, job: &Job) -> Result<Page, ()> {
    let count = doc.n_pages().max(0) as usize;
    if count == 0 {
        return Err(());
    }
    let index = job.index.min(count - 1);
    let page = doc.page(index as i32).ok_or(())?;
    let (pw, ph) = page.size();
    if pw <= 0.0 || ph <= 0.0 {
        return Err(());
    }
    let fit = (job.frame.0 as f64 / pw).min(job.frame.1 as f64 / ph).max(0.01);
    let scale = fit * job.zoom.clamp(MIN_ZOOM, MAX_ZOOM) as f64;
    // A page is never rendered larger than 8192 px a side, whatever the zoom asks for.
    let (w, h) = (((pw * scale).round() as i32).clamp(1, 8192), ((ph * scale).round() as i32).clamp(1, 8192));
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, w, h).map_err(|_| ())?;
    {
        let ctx = cairo::Context::new(&surface).map_err(|_| ())?;
        ctx.set_source_rgb(1.0, 1.0, 1.0);
        ctx.paint().map_err(|_| ())?;
        ctx.scale(w as f64 / pw, h as f64 / ph);
        page.render(&ctx);
    }
    surface.flush();
    let stride = surface.stride() as usize;
    let data = surface.data().map_err(|_| ())?;
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h as usize {
        for px in data[y * stride..y * stride + w as usize * 4].chunks_exact(4) {
            // ARGB32 is native-endian, so on this little-endian box the bytes are B, G, R, A, and the
            // white ground makes every pixel opaque, so premultiplication changes nothing.
            rgba.extend_from_slice(&[px[2], px[1], px[0], 255]);
        }
    }
    Ok(Page { path: job.path.clone(), index, count, width: w as u32, height: h as u32, rgba })
}

// Blocking: the caller runs this off the UI thread and the worker answers in order.
pub fn page(path: &str, index: usize, frame: (f32, f32), zoom: f32) -> Result<Page, ()> {
    let (reply, rx) = mpsc::channel();
    worker().send(Job { path: path.to_string(), index, frame, zoom, reply }).map_err(|_| ())?;
    rx.recv().map_err(|_| ())?
}

pub fn zoom_by(zoom: f32, steps: i32) -> f32 {
    (zoom + steps as f32 * ZOOM_STEP).clamp(MIN_ZOOM, MAX_ZOOM)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_zoom_ladder_clamps_at_both_rungs_and_a_path_is_escaped() {
        assert_eq!(zoom_by(1.0, -1), 1.0);
        assert_eq!(zoom_by(1.0, 3), 1.75);
        assert_eq!(zoom_by(3.9, 4), 4.0);
        assert_eq!(gio_uri("/a b/c#d.pdf"), "file:///a%20b/c%23d.pdf");
    }

    // A real two-page PDF written by cairo itself, then read back through poppler.
    #[test]
    fn a_page_renders_fitted_to_the_frame_and_turns() {
        let path = std::env::temp_dir().join(format!("flea-iced-{}.pdf", std::process::id()));
        {
            let surface = cairo::PdfSurface::new(200.0, 100.0, &path).unwrap();
            let ctx = cairo::Context::new(&surface).unwrap();
            ctx.set_source_rgb(1.0, 0.0, 0.0);
            ctx.paint().unwrap();
            ctx.show_page().unwrap();
            ctx.set_source_rgb(0.0, 0.0, 1.0);
            ctx.paint().unwrap();
            ctx.show_page().unwrap();
            surface.finish();
        }
        let p = path.to_str().unwrap();
        let first = page(p, 0, (400.0, 400.0), 1.0).unwrap();
        assert_eq!((first.count, first.width, first.height), (2, 400, 200));
        let mid = |pg: &Page| { let i = ((pg.height / 2 * pg.width + pg.width / 2) * 4) as usize; [pg.rgba[i], pg.rgba[i + 1], pg.rgba[i + 2]] };
        assert_eq!(mid(&first), [255, 0, 0]);
        let second = page(p, 1, (400.0, 400.0), 2.0).unwrap();
        assert_eq!((second.index, second.width), (1, 800));
        assert_eq!(mid(&second), [0, 0, 255]);
        // Past the last page clamps to it; a file that is not a PDF fails.
        assert_eq!(page(p, 9, (100.0, 100.0), 1.0).unwrap().index, 1);
        assert!(page("/nonexistent/x.pdf", 0, (100.0, 100.0), 1.0).is_err());
        let _ = std::fs::remove_file(&path);
    }
}
