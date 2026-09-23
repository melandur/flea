// flea --sheet <path> <rows>: a spreadsheet's first sheet as CSV on stdout, for the preview's table.
// ui/PreviewTable.qml runs it and parses the answer with the same reader a .csv gets, so the grid,
// its widths and its alignment are one code path whatever the file was. The package is a zip, and
// bsdtar, which the archive preview already needs, is what opens it: no zip crate, no inflate here.
// bsdtar parses the untrusted package, so it runs in the same jail as every other archive tool, and
// the reader here is held to a byte budget, because a few kilobytes of shared strings or ODF repeats
// can otherwise expand into gigabytes of cells.
use crate::backend::sandbox;
use crate::{sheetods, sheetxlsx};
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};

const BSDTAR: &str = "bsdtar";
// A part bigger than this once inflated is not read to the end: a zip bomb stops here, and a real
// sheet that large is still previewed as far as it got.
const MAX_PART_BYTES: usize = 256 * 1024 * 1024;
// Columns past this are not written; the table draws fewer still.
const MAX_COLUMNS: usize = 256;
const MAX_ROWS: usize = 1_000_000;
// One cell shows at most this much; the table elides far sooner, and a longer cell is a padding attack.
pub const MAX_CELL_CHARS: usize = 4096;
// The whole answer stops growing here, whatever the rows and columns would still allow.
const MAX_OUTPUT_BYTES: usize = 64 * 1024 * 1024;

// What a reader may produce, so an 8 KB workbook cannot answer with 4 GB of copies of one string.
#[derive(Clone, Copy)]
pub struct Limits {
    pub rows: usize,
    pub columns: usize,
    pub cell_chars: usize,
    pub bytes: usize,
}

impl Limits {
    pub fn rows(rows: usize) -> Limits {
        Limits { rows, columns: MAX_COLUMNS, cell_chars: MAX_CELL_CHARS, bytes: MAX_OUTPUT_BYTES }
    }
}

// A cell cut to the budget at a character boundary, marked so a cut is never mistaken for the value.
pub fn clip(mut cell: String, max_chars: usize) -> String {
    if let Some((at, _)) = cell.char_indices().nth(max_chars) {
        cell.truncate(at);
        cell.push('\u{2026}');
    }
    cell
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Format {
    Xlsx,
    Ods,
    Fods,
}

// Which reader a name gets; anything else is not a sheet this knows.
pub fn format_of(name: &str) -> Option<Format> {
    let lower = name.to_ascii_lowercase();
    let ext = lower.rsplit('.').next().unwrap_or("");
    match ext {
        "xlsx" | "xlsm" | "xltx" | "xltm" => Some(Format::Xlsx),
        "ods" | "ots" => Some(Format::Ods),
        "fods" => Some(Format::Fods),
        _ => None,
    }
}

pub fn command(args: &[String]) -> i32 {
    let (path, limit) = match args {
        [_, _, path, rows] => match rows.parse::<usize>() {
            Ok(n) if n > 0 => (path, n.min(MAX_ROWS)),
            _ => return refuse("--sheet takes a path and a row count"),
        },
        _ => return refuse("--sheet takes a path and a row count"),
    };
    let rows = match read(Path::new(path), limit) {
        Ok(rows) => rows,
        Err(why) => return refuse(&format!("{}: {}", path, why)),
    };
    let mut out = std::io::stdout().lock();
    // A reader that has gone away has what it needed, so a closed pipe is not an error.
    let _ = out.write_all(csv(&rows).as_bytes());
    0
}

fn refuse(message: &str) -> i32 {
    eprintln!("flea: {}", message);
    1
}

pub fn read(path: &Path, limit: usize) -> Result<Vec<Vec<String>>, String> {
    let format = format_of(&path.to_string_lossy()).ok_or("not a spreadsheet this can read")?;
    // The jail binds the input by its real path, so a relative name or a symlink is resolved first.
    let resolved = std::fs::canonicalize(path).map_err(|e| e.to_string())?;
    let path = resolved.as_path();
    // Fail closed, the rule archivework.rs follows: without bwrap and prlimit no package is opened.
    if format != Format::Fods && !sandbox::available() {
        return Err("the sandbox is unavailable: bwrap or prlimit is not on PATH".into());
    }
    // Enough closing tags to fill the rows asked for, with room for the empty rows ODF writes as
    // their own elements, so a huge sheet is inflated only as far as the preview looks.
    let enough = limit.saturating_mul(4).max(1000);
    match format {
        Format::Xlsx => {
            let workbook = member(path, "xl/workbook.xml", None).ok_or("no workbook in the package")?;
            let rels = member(path, "xl/_rels/workbook.xml.rels", None).unwrap_or_default();
            let book = sheetxlsx::Book {
                strings: member(path, "xl/sharedStrings.xml", None).map(|x| sheetxlsx::shared_strings(&x)).unwrap_or_default(),
                date_styles: member(path, "xl/styles.xml", None).map(|x| sheetxlsx::date_styles(&x)).unwrap_or_default(),
                date1904: sheetxlsx::date1904(&workbook),
            };
            let sheet = member(path, &sheetxlsx::first_sheet(&workbook, &rels), Some(("row", enough))).ok_or("no first sheet in the package")?;
            Ok(sheetxlsx::rows(&sheet, &book, Limits::rows(limit)))
        }
        Format::Ods => {
            let content = member(path, "content.xml", Some(("table-row", enough))).ok_or("no content in the package")?;
            Ok(sheetods::rows(&content, Limits::rows(limit)))
        }
        Format::Fods => {
            let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
            let mut bytes = Vec::new();
            file.take(MAX_PART_BYTES as u64).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
            Ok(sheetods::rows(&String::from_utf8_lossy(&bytes), Limits::rows(limit)))
        }
    }
}

// One member of the package, inflated by a jailed bsdtar to a pipe: the package is bound read-only
// and nothing is writable, the boundary sandbox::wrap_readonly gives ffprobe. With stop, reading
// ends once that many elements of that local name have closed, and bsdtar is killed rather than
// left to inflate the rest.
fn member(path: &Path, name: &str, stop: Option<(&str, usize)>) -> Option<String> {
    let inner = [BSDTAR, "-xOf", &path.to_string_lossy(), "--", name].map(String::from);
    let full = sandbox::wrap_readonly(&inner, path);
    let mut child = Command::new(&full[0]).args(&full[1..])
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null())
        .spawn().ok()?;
    let mut pipe = child.stdout.take()?;
    let (mut bytes, mut chunk) = (Vec::new(), vec![0u8; 1 << 16]);
    let (mut scanned, mut closed, mut cut) = (0usize, 0usize, false);
    loop {
        let n = match pipe.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        bytes.extend_from_slice(&chunk[..n]);
        if let Some((local, want)) = stop {
            let (count, upto) = closes(&bytes, scanned, local);
            closed += count;
            scanned = upto;
            if closed >= want {
                cut = true;
                break;
            }
        }
        if bytes.len() >= MAX_PART_BYTES {
            cut = true;
            break;
        }
    }
    drop(pipe);
    if cut {
        let _ = child.kill();
    }
    let status = child.wait().ok()?;
    if !cut && !status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

// A closing tag's name is never longer than this; "</" with no ">" this soon is not a tag at all.
const MAX_TAG_NAME: usize = 256;

// How many </x:local> or </local> tags close in bytes[from..], and where the scan stopped: before a
// tag the buffer cuts off, so the next chunk reads it whole and nothing is counted twice. Only a
// short tail is ever left for the next chunk, so a "</" with no ">" cannot make every chunk rescan
// the rest of the part, which cost 110 s of CPU on a 254 KB package.
pub fn closes(bytes: &[u8], from: usize, local: &str) -> (usize, usize) {
    let (mut count, mut i) = (0usize, from);
    while let Some(at) = bytes[i..].windows(2).position(|w| w == b"</") {
        let start = i + at + 2;
        let window = &bytes[start..bytes.len().min(start + MAX_TAG_NAME)];
        let Some(len) = window.iter().position(|&b| b == b'>') else {
            if window.len() < MAX_TAG_NAME {
                return (count, i + at);
            }
            i = start;
            continue;
        };
        let name = &bytes[start..start + len];
        let tail = name.rsplit(|&b| b == b':').next().unwrap_or(name);
        if tail == local.as_bytes() {
            count += 1;
        }
        i = start + len + 1;
    }
    // A lone '<' at the very end could be the first half of the next "</".
    (count, if bytes.len() > i && bytes[bytes.len() - 1] == b'<' { bytes.len() - 1 } else { bytes.len().max(i) })
}

// RFC 4180: a field is quoted when it holds the separator, a quote or a line break, and its own
// quotes are doubled; records end with \n.
pub fn csv(rows: &[Vec<String>]) -> String {
    let mut out = String::new();
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            if cell.contains([',', '"', '\n', '\r']) {
                out.push('"');
                out.push_str(&cell.replace('"', "\"\""));
                out.push('"');
            } else {
                out.push_str(cell);
            }
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
#[path = "sheet_tests.rs"]
mod tests;
