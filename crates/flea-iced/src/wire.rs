// The backend's line protocol as this front end reads it; docs/protocol.md is the contract.
// Only the lines the listing pane consumes are typed. Everything else is Reply::Other, which is
// dropped rather than misread, the same way ui/js/Messages.js ignores a `t` it does not route.
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Row {
    pub n: String,
    pub d: bool,
    #[serde(default)]
    pub s: u64,
    #[serde(default)]
    pub m: i64,
    #[serde(default)]
    pub p: u32,
    #[serde(default)]
    pub i: String,
    #[serde(default)]
    pub t: bool,
    #[serde(default)]
    pub k: usize,
    #[serde(default)]
    pub l: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Reply {
    Listed { n: usize },
    Rows { start: usize, rows: Vec<Row>, kinds: Vec<String> },
    // `sizes` absent reads as true, per docs/protocol.md "changed".
    Changed { path: String },
    Located { directory: String, path: String, index: i64 },
    Error { r#where: String, path: String, msg: String },
    Paths { paths: Vec<String> },
    TransferStarted { id: u64, n: usize, moving: bool },
    TransferQueued { id: u64, n: usize, moving: bool, ahead: usize },
    TransferProgress { id: u64, index: usize, name: String, bytes: u64, total: u64 },
    TransferItem { id: u64, index: usize, name: String, ok: bool, err: String },
    TransferDone { id: u64, ok: usize, failed: usize, skipped: usize, cancelled: bool },
    Trashed { ok: usize, failed: usize },
    Renamed { path: String },
    Made { path: String },
    Undone { op: String },
    // file is empty rather than absent on failure, so a row that was asked for always answers.
    Thumbed { row: usize, file: String },
    Other,
}

pub fn parse(line: &str) -> Option<Reply> {
    let v: Value = serde_json::from_str(line).ok()?;
    let t = v.get("t")?.as_str()?;
    let field = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let uint = |k: &str| v.get(k).and_then(Value::as_u64);
    let flag = |k: &str| v.get(k).and_then(Value::as_bool).unwrap_or(false);
    Some(match t {
        "listed" => Reply::Listed { n: v.get("n")?.as_u64()? as usize },
        "rows" => Reply::Rows {
            start: v.get("start")?.as_u64()? as usize,
            rows: serde_json::from_value(v.get("rows")?.clone()).ok()?,
            kinds: serde_json::from_value(v.get("kinds").cloned().unwrap_or(json!([]))).ok()?,
        },
        "changed" => Reply::Changed { path: field("path") },
        // The single-path form only; the batch form carries `matches` and no `index`.
        "located" if v.get("index").is_some() => Reply::Located {
            directory: field("directory"),
            path: field("path"),
            index: v.get("index")?.as_i64()?,
        },
        "paths" => Reply::Paths { paths: serde_json::from_value(v.get("paths")?.clone()).ok()? },
        "transferstarted" => Reply::TransferStarted { id: uint("id")?, n: uint("n")? as usize, moving: flag("moving") },
        "transferqueued" => Reply::TransferQueued {
            id: uint("id")?,
            n: uint("n")? as usize,
            moving: flag("moving"),
            ahead: uint("ahead").unwrap_or(0) as usize,
        },
        "transferprogress" => Reply::TransferProgress {
            id: uint("id")?,
            index: uint("index")? as usize,
            name: field("name"),
            bytes: uint("bytes").unwrap_or(0),
            total: uint("total").unwrap_or(0),
        },
        // err rides only on a failure, so its absence is the empty string.
        "transferitem" => Reply::TransferItem {
            id: uint("id")?,
            index: uint("index")? as usize,
            name: field("name"),
            ok: flag("ok"),
            err: field("err"),
        },
        "transferdone" => Reply::TransferDone {
            id: uint("id")?,
            ok: uint("ok").unwrap_or(0) as usize,
            failed: uint("failed").unwrap_or(0) as usize,
            skipped: uint("skipped").unwrap_or(0) as usize,
            cancelled: flag("cancelled"),
        },
        "trashed" => Reply::Trashed { ok: uint("ok").unwrap_or(0) as usize, failed: uint("failed").unwrap_or(0) as usize },
        // A refusal is an error line, never these with ok false, so ok is not read.
        "renamed" => Reply::Renamed { path: field("path") },
        "made" => Reply::Made { path: field("path") },
        "undone" => Reply::Undone { op: field("op") },
        "thumbed" => Reply::Thumbed { row: uint("row")? as usize, file: field("file") },
        "error" => Reply::Error { r#where: field("where"), path: field("path"), msg: field("msg") },
        _ => Reply::Other,
    })
}

// The ordering fields every list carries, as ui/Backend.qml listRequest composes them.
#[derive(Debug, Clone, PartialEq)]
pub struct Order {
    pub by: String,
    pub desc: bool,
    pub folders_first: bool,
    pub group_by_kind: bool,
}

impl Default for Order {
    fn default() -> Self {
        Order { by: "name".into(), desc: false, folders_first: true, group_by_kind: false }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    List { path: String, first: usize, hidden: bool, order: Order },
    Window { start: usize, count: usize },
    Locate { path: String },
    // rows, not paths: a selection can be wider than the window the client holds.
    Paths { rows: Vec<usize> },
    Transfer { moving: bool, paths: Vec<String>, dest: String },
    TransferCancel { id: u64 },
    Trash { rows: Vec<usize> },
    Rename { path: String, to: String },
    // No name: the backend answers with the first free "New Folder".
    Mkdir { path: String },
    Undo,
    // Never sent with an empty rows: thumbcancel with none cancels everything queued.
    Thumb { rows: Vec<usize> },
    ThumbCancel { rows: Vec<usize> },
    Quit,
}

impl Request {
    pub fn line(&self) -> String {
        let v = match self {
            Request::List { path, first, hidden, order } => json!({
                "c": "list", "path": path, "first": first, "hidden": hidden, "by": order.by,
                "desc": order.desc, "foldersFirst": order.folders_first, "groupByKind": order.group_by_kind,
            }),
            Request::Window { start, count } => json!({"c": "window", "start": start, "count": count}),
            Request::Locate { path } => json!({"c": "locate", "path": path}),
            Request::Paths { rows } => json!({"c": "paths", "rows": rows}),
            // Anything but exactly "move" copies, so a malformed request can never remove a source.
            Request::Transfer { moving, paths, dest } => json!({
                "c": "transfer", "op": if *moving { "move" } else { "copy" }, "paths": paths, "dest": dest,
            }),
            Request::TransferCancel { id } => json!({"c": "transfercancel", "id": id}),
            Request::Trash { rows } => json!({"c": "trash", "rows": rows}),
            Request::Rename { path, to } => json!({"c": "rename", "path": path, "to": to}),
            Request::Mkdir { path } => json!({"c": "mkdir", "path": path}),
            Request::Undo => json!({"c": "undo"}),
            Request::Thumb { rows } => json!({"c": "thumb", "rows": rows}),
            Request::ThumbCancel { rows } => json!({"c": "thumbcancel", "rows": rows}),
            Request::Quit => json!({"c": "quit"}),
        };
        format!("{v}\n")
    }
}

#[cfg(test)]
#[path = "wire_tests.rs"]
mod tests;
