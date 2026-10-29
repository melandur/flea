// The Convert popup, as ui/ConvertDialog.qml and ui/js/Convert.js hold it: the formats ImageMagick
// writes here, the one that starts selected, the destination name, whether to strip metadata, and the
// `convert` check that refuses a name already on disk before anything is written. Toolkit free.
use serde_json::{json, Value};

// What the popup offers; every one is in this box's ImageMagick delegate list.
pub const FORMATS: &[&str] = &["jpg", "png", "webp", "avif", "heic", "tiff", "bmp"];

pub fn label(format: &str) -> String {
    match format {
        "jpg" => "JPEG".into(),
        "png" => "PNG".into(),
        "webp" => "WebP".into(),
        "avif" => "AVIF".into(),
        "heic" => "HEIC".into(),
        "tiff" => "TIFF".into(),
        "bmp" => "BMP".into(),
        other => other.to_uppercase(),
    }
}

fn split(name: &str) -> (&str, String) {
    match name.rfind('.') {
        Some(cut) if cut > 0 => (&name[..cut], name[cut + 1..].to_lowercase()),
        _ => (name, String::new()),
    }
}

// Never the file it came from: a real conversion is "(converted)", a same-format strip "(stripped)".
pub fn dest_name(name: &str, format: &str) -> String {
    let (stem, ext) = split(name);
    let word = if ext == format { "(stripped)" } else { "(converted)" };
    format!("{stem} {word}.{format}")
}

// The first format that is not the one the file already is.
pub fn default_format(name: &str) -> usize {
    let (_, ext) = split(name);
    FORMATS.iter().position(|f| *f != ext).unwrap_or(0)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Convert {
    pub path: String,
    pub name: String,
    pub menu_id: u64,
    pub cursor: usize,
    pub strip: bool,
    // 0 the formats, 1 the metadata box, 2 Cancel, 3 Convert.
    pub focus: u8,
    pub checking: bool,
    check_again: bool,
    pub collision: bool,
    pub error: String,
    pub busy: bool,
    serial: u64,
    request_id: u64,
    operation: u64,
}

impl Convert {
    pub fn open(path: &str, name: &str, menu_id: u64) -> (Convert, Vec<Value>) {
        let mut c = Convert {
            path: path.into(),
            name: name.into(),
            menu_id,
            cursor: default_format(name),
            strip: false,
            focus: 0,
            checking: false,
            check_again: false,
            collision: false,
            error: String::new(),
            busy: false,
            serial: 0,
            request_id: 0,
            operation: 0,
        };
        let out = c.probe();
        (c, out)
    }

    pub fn format(&self) -> &'static str {
        FORMATS[self.cursor]
    }

    pub fn output(&self) -> String {
        let parent = &self.path[..self.path.rfind('/').map(|i| i + 1).unwrap_or(0)];
        format!("{parent}{}", dest_name(&self.name, self.format()))
    }

    pub fn can_convert(&self) -> bool {
        !self.busy && !self.checking && !self.collision && self.error.is_empty()
    }

    fn request(&self, check: bool) -> Value {
        json!({"c": "convert", "path": self.path, "dest": self.output(), "strip": self.strip, "menuId": self.menu_id, "requestId": self.request_id, "check": check})
    }

    fn probe(&mut self) -> Vec<Value> {
        if self.busy {
            return vec![];
        }
        if self.checking {
            self.check_again = true;
            return vec![];
        }
        self.checking = true;
        self.error.clear();
        self.collision = false;
        self.serial += 1;
        self.request_id = self.serial;
        vec![self.request(true)]
    }

    pub fn choose(&mut self, index: usize) -> Vec<Value> {
        if self.busy {
            return vec![];
        }
        self.focus = 0;
        let next = index.min(FORMATS.len() - 1);
        if next != self.cursor || !self.error.is_empty() {
            self.cursor = next;
            return self.probe();
        }
        vec![]
    }

    pub fn toggle_strip(&mut self) {
        if !self.busy {
            self.strip = !self.strip;
        }
    }

    pub fn step_focus(&mut self, back: bool) {
        let parts: &[u8] = if self.can_convert() { &[0, 1, 2, 3] } else { &[0, 1, 2] };
        let at = parts.iter().position(|p| *p == self.focus).unwrap_or(0);
        let n = parts.len();
        self.focus = parts[(at + if back { n - 1 } else { 1 }) % n];
    }

    pub fn commit(&mut self) -> Vec<Value> {
        if !self.can_convert() {
            return vec![];
        }
        self.busy = true;
        self.operation = 0;
        self.serial += 1;
        self.request_id = self.serial;
        vec![self.request(false)]
    }

    fn ours(&self, v: &Value) -> bool {
        v.get("requestId").and_then(Value::as_u64) == Some(self.request_id) && v.get("source").and_then(Value::as_str) == Some(self.path.as_str())
    }

    pub fn checked(&mut self, v: &Value) -> Vec<Value> {
        if !self.checking || !self.ours(v) {
            return vec![];
        }
        self.checking = false;
        if self.check_again || v.get("path").and_then(Value::as_str) != Some(self.output().as_str()) {
            self.check_again = false;
            return self.probe();
        }
        self.collision = v.get("collision") == Some(&json!(true));
        let ok = v.get("ok") == Some(&json!(true));
        self.error = if ok && !self.collision { String::new() } else { v.get("error").and_then(Value::as_str).unwrap_or("").to_string() };
        vec![]
    }

    pub fn started(&mut self, v: &Value) {
        if self.busy && self.ours(v) {
            self.operation = v.get("id").and_then(Value::as_u64).unwrap_or(0);
        }
    }

    // None for an answer that is not this popup's; Some(true) when it converted and closes, and
    // Some(false) for a refusal, which keeps it open with the reason.
    pub fn done(&mut self, v: &Value) -> Option<bool> {
        let ok = v.get("ok") == Some(&json!(true));
        let id = v.get("id").and_then(Value::as_u64).unwrap_or(0);
        // ConvertDialog.onConvertDone: once started, only that operation's answer counts; before it,
        // only a refusal can be ours, the one the backend answers without starting anything.
        let stale = if self.operation != 0 { id != self.operation } else { ok };
        if !self.busy || !self.ours(v) || stale {
            return None;
        }
        self.busy = false;
        self.collision = v.get("collision") == Some(&json!(true));
        self.error = v.get("err").and_then(Value::as_str).unwrap_or("").to_string();
        Some(ok)
    }
}

#[cfg(test)]
#[path = "convertcard_tests.rs"]
mod tests;
