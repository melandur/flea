// flea --sheet <path> <rows>: a spreadsheet's first sheet as CSV on stdout, for the preview's table.
// ui/PreviewTable.qml runs it and parses the answer with the same reader a .csv gets, so the grid,
// its widths and its alignment are one code path whatever the file was. The package is a zip, and
// bsdtar, which the archive preview already needs, is what opens it: no zip crate, no inflate here.
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
            Ok(sheetxlsx::rows(&sheet, &book, limit, MAX_COLUMNS))
        }
        Format::Ods => {
            let content = member(path, "content.xml", Some(("table-row", enough))).ok_or("no content in the package")?;
            Ok(sheetods::rows(&content, limit, MAX_COLUMNS))
        }
        Format::Fods => {
            let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
            let mut bytes = Vec::new();
            file.take(MAX_PART_BYTES as u64).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
            Ok(sheetods::rows(&String::from_utf8_lossy(&bytes), limit, MAX_COLUMNS))
        }
    }
}

// One member of the package, inflated by bsdtar to a pipe. With stop, reading ends once that many
// elements of that local name have closed, and bsdtar is killed rather than left to inflate the rest.
fn member(path: &Path, name: &str, stop: Option<(&str, usize)>) -> Option<String> {
    let mut child = Command::new(BSDTAR)
        .arg("-xOf").arg(path).arg("--").arg(name)
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

// How many </x:local> or </local> tags close in bytes[from..], and where the scan stopped: before a
// tag the buffer cuts off, so the next chunk reads it whole and nothing is counted twice.
pub fn closes(bytes: &[u8], from: usize, local: &str) -> (usize, usize) {
    let (mut count, mut i) = (0usize, from);
    while let Some(at) = bytes[i..].windows(2).position(|w| w == b"</") {
        let start = i + at + 2;
        let Some(len) = bytes[start..].iter().position(|&b| b == b'>') else {
            return (count, i + at);
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
