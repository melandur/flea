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
            Request::Quit => json!({"c": "quit"}),
        };
        format!("{v}\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rows_sample_from_the_protocol_parses() {
        let line = r#"{"t":"rows","start":0,"rows":[{"n":"say \"hi\".txt","d":false,"s":12,"m":1787790423,"p":33188,"i":"text-x-generic","t":false,"k":0},{"n":"photos","d":true,"s":4096,"m":1787790424,"p":16877,"i":"folder","t":false,"k":1,"v":56}],"kinds":["Plain text document","Folder"],"ms":1.250}"#;
        let Some(Reply::Rows { start, rows, kinds }) = parse(line) else { panic!("not rows") };
        assert_eq!(start, 0);
        assert_eq!(rows[0].n, "say \"hi\".txt");
        assert!(rows[1].d);
        assert_eq!(kinds, vec!["Plain text document", "Folder"]);
    }

    #[test]
    fn an_unknown_line_is_other_and_junk_is_none() {
        assert_eq!(parse(r#"{"t":"thumbed","row":2,"file":"/x.png","ms":1.0}"#), Some(Reply::Other));
        assert_eq!(parse("not json"), None);
    }

    #[test]
    fn the_write_replies_from_the_protocol_parse() {
        assert_eq!(
            parse(r#"{"t":"transferitem","id":12,"index":0,"name":"a.txt","ok":true}"#),
            Some(Reply::TransferItem { id: 12, index: 0, name: "a.txt".into(), ok: true, err: String::new() })
        );
        assert_eq!(
            parse(r#"{"t":"transferdone","id":12,"ok":1,"failed":1,"skipped":0,"cancelled":false}"#),
            Some(Reply::TransferDone { id: 12, ok: 1, failed: 1, skipped: 0, cancelled: false })
        );
        assert_eq!(
            parse(r#"{"t":"transferqueued","id":13,"n":1,"moving":false,"ahead":1}"#),
            Some(Reply::TransferQueued { id: 13, n: 1, moving: false, ahead: 1 })
        );
        assert_eq!(parse(r#"{"t":"made","ok":true,"path":"/home/gm/New Folder"}"#), Some(Reply::Made { path: "/home/gm/New Folder".into() }));
        assert_eq!(parse(r#"{"t":"paths","paths":["/a","/b"]}"#), Some(Reply::Paths { paths: vec!["/a".into(), "/b".into()] }));
    }

    #[test]
    fn a_transfer_says_move_only_when_it_moves() {
        let l = Request::Transfer { moving: false, paths: vec!["/a".into()], dest: "/d".into() }.line();
        assert!(l.contains(r#""op":"copy""#));
        let l = Request::Transfer { moving: true, paths: vec!["/a".into()], dest: "/d".into() }.line();
        assert!(l.contains(r#""op":"move""#));
    }

    #[test]
    fn a_request_is_one_line_with_its_newline() {
        let l = Request::List { path: "/a \"b\"".into(), first: 350, hidden: false, order: Order::default() }.line();
        assert!(l.ends_with('\n') && l.matches('\n').count() == 1);
        let v: Value = serde_json::from_str(l.trim_end()).unwrap();
        assert_eq!(v["path"], "/a \"b\"");
        assert_eq!(v["first"], 350);
    }
}
