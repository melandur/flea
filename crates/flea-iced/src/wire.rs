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
    Other,
}

pub fn parse(line: &str) -> Option<Reply> {
    let v: Value = serde_json::from_str(line).ok()?;
    let t = v.get("t")?.as_str()?;
    let field = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
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
    fn a_request_is_one_line_with_its_newline() {
        let l = Request::List { path: "/a \"b\"".into(), first: 350, hidden: false, order: Order::default() }.line();
        assert!(l.ends_with('\n') && l.matches('\n').count() == 1);
        let v: Value = serde_json::from_str(l.trim_end()).unwrap();
        assert_eq!(v["path"], "/a \"b\"");
        assert_eq!(v["first"], 350);
    }
}
