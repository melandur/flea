// An .xlsx read far enough to preview: the first sheet's cells as text, in their grid positions,
// shared strings looked up and dates shown as dates. The parts come in as strings, so nothing here
// knows the package is a zip; src/sheet.rs pulls them out.
use crate::sheet::{clip, Limits};
use crate::sheetxml::{attr, decode, tokens, Tok};
use std::collections::HashMap;

pub struct Book {
    pub strings: Vec<String>,
    // One flag per cellXfs entry, in order, which is what a cell's s="" indexes.
    pub date_styles: Vec<bool>,
    pub date1904: bool,
}

// Where the first sheet lives: the workbook names its sheets in tab order by relationship id, and the
// relationships part says which member each id is. A workbook that says nothing gets the usual name.
pub fn first_sheet(workbook: &str, rels: &str) -> String {
    let id = tokens(workbook).find_map(|t| match t {
        Tok::Open { name: "sheet", attrs, .. } => attr(attrs, "id"),
        _ => None,
    });
    let target = id.and_then(|id| {
        tokens(rels).find_map(|t| match t {
            Tok::Open { name: "Relationship", attrs, .. } if attr(attrs, "Id").as_deref() == Some(id.as_str()) => attr(attrs, "Target"),
            _ => None,
        })
    });
    match target {
        Some(t) if t.starts_with('/') => t[1..].to_string(),
        Some(t) => format!("xl/{}", t),
        None => "xl/worksheets/sheet1.xml".to_string(),
    }
}

// A Mac workbook can count its serial dates from 1904 instead of 1900.
pub fn date1904(workbook: &str) -> bool {
    tokens(workbook).any(|t| match t {
        Tok::Open { name: "workbookPr", attrs, .. } => matches!(attr(attrs, "date1904").as_deref(), Some("1") | Some("true")),
        _ => false,
    })
}

// Each <si> is one string, plain or in rich runs; a phonetic guide (<rPh>) is not part of what shows.
pub fn shared_strings(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let (mut cur, mut in_t, mut in_rph) = (String::new(), false, false);
    for t in tokens(xml) {
        match t {
            Tok::Open { name: "si", empty, .. } => {
                cur.clear();
                if empty {
                    out.push(String::new());
                }
            }
            Tok::Open { name: "t", empty: false, .. } => in_t = true,
            Tok::Open { name: "rPh", empty: false, .. } => in_rph = true,
            Tok::Close("t") => in_t = false,
            Tok::Close("rPh") => in_rph = false,
            Tok::Close("si") => out.push(std::mem::take(&mut cur)),
            Tok::Text(s) if in_t && !in_rph => cur.push_str(&decode(s)),
            Tok::Cdata(s) if in_t && !in_rph => cur.push_str(s),
            _ => {}
        }
    }
    out
}

// Which cell styles are dates: a built-in date format by its id, or a custom code that spells one.
pub fn date_styles(xml: &str) -> Vec<bool> {
    let mut custom: HashMap<u32, bool> = HashMap::new();
    let (mut out, mut in_xfs) = (Vec::new(), false);
    for t in tokens(xml) {
        match t {
            Tok::Open { name: "numFmt", attrs, .. } => {
                if let (Some(id), Some(code)) = (attr(attrs, "numFmtId").and_then(|v| v.parse().ok()), attr(attrs, "formatCode")) {
                    custom.insert(id, is_date_code(&code));
                }
            }
            Tok::Open { name: "cellXfs", empty: false, .. } => in_xfs = true,
            Tok::Close("cellXfs") => in_xfs = false,
            Tok::Open { name: "xf", attrs, .. } if in_xfs => {
                let id: u32 = attr(attrs, "numFmtId").and_then(|v| v.parse().ok()).unwrap_or(0);
                out.push(is_date_id(id) || custom.get(&id).copied().unwrap_or(false));
            }
            _ => {}
        }
    }
    out
}

// ECMA-376 18.8.30's built-in date and time formats, with the East Asian ones Excel adds.
fn is_date_id(id: u32) -> bool {
    matches!(id, 14..=22 | 27..=36 | 45..=47 | 50..=58)
}

// A format code is a date when a d, m, y, h or s survives once its quoted text, its [colour] and
// [locale] brackets and its backslash escapes are taken out; "0.00" and "General" have none.
pub fn is_date_code(code: &str) -> bool {
    let (mut quoted, mut bracket, mut escaped) = (false, false, false);
    for c in code.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '"' => quoted = !quoted,
            _ if quoted => {}
            '\\' => escaped = true,
            '[' => bracket = true,
            ']' => bracket = false,
            _ if bracket => {}
            'd' | 'D' | 'm' | 'M' | 'y' | 'Y' | 'h' | 'H' | 's' | 'S' => return true,
            _ => {}
        }
    }
    false
}

// The first limit rows of a sheet, each padded out to its cells' columns; a row the file skips is an
// empty row here, so the preview's row numbers are the sheet's own.
pub fn rows(sheet: &str, book: &Book, limits: Limits) -> Vec<Vec<String>> {
    let (limit, max_cols) = (limits.rows, limits.columns);
    let mut spent = 0usize;
    let mut out: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let (mut col, mut kind, mut style, mut value) = (0usize, String::new(), 0usize, String::new());
    let (mut in_v, mut in_t, mut in_rph) = (false, false, false);
    for t in tokens(sheet) {
        if out.len() >= limit || spent >= limits.bytes {
            break;
        }
        match t {
            Tok::Open { name: "row", attrs, empty } => {
                let r: usize = attr(attrs, "r").and_then(|v| v.parse().ok()).unwrap_or(out.len() + 1);
                while out.len() + 1 < r && out.len() < limit {
                    out.push(Vec::new());
                }
                row.clear();
                if empty {
                    out.push(Vec::new());
                }
            }
            Tok::Close("row") => out.push(std::mem::take(&mut row)),
            Tok::Open { name: "c", attrs, empty } => {
                col = attr(attrs, "r").map(|r| column_of(&r)).unwrap_or(row.len());
                kind = attr(attrs, "t").unwrap_or_default();
                style = attr(attrs, "s").and_then(|v| v.parse().ok()).unwrap_or(0);
                value.clear();
                if empty {
                    col = usize::MAX;
                }
            }
            Tok::Open { name: "v", empty: false, .. } => in_v = true,
            Tok::Open { name: "t", empty: false, .. } => in_t = true,
            Tok::Open { name: "rPh", empty: false, .. } => in_rph = true,
            Tok::Close("v") => in_v = false,
            Tok::Close("t") => in_t = false,
            Tok::Close("rPh") => in_rph = false,
            Tok::Text(s) if (in_v || (in_t && !in_rph)) && value.len() < limits.bytes => value.push_str(&decode(s)),
            Tok::Cdata(s) if (in_v || (in_t && !in_rph)) && value.len() < limits.bytes => value.push_str(s),
            Tok::Close("c") if col < max_cols => {
                if row.len() <= col {
                    row.resize(col + 1, String::new());
                }
                let cell = clip(shown(&kind, &value, style, book), limits.cell_chars);
                spent += cell.len() + 1;
                row[col] = cell;
            }
            Tok::Close("sheetData") => break,
            _ => {}
        }
    }
    // Excel writes a formatted row whether or not it holds anything, so a sheet styled to row 1000
    // would end in hundreds of blank records; the ones before real data stay, as their row numbers.
    while out.last().is_some_and(|r| r.iter().all(String::is_empty)) {
        out.pop();
    }
    out
}

// "B3" is column 1: the letters are base 26 with no zero digit.
pub fn column_of(reference: &str) -> usize {
    let n = reference.bytes().take_while(|b| b.is_ascii_alphabetic())
        .fold(0usize, |n, b| n.saturating_mul(26).saturating_add((b.to_ascii_uppercase() - b'A' + 1) as usize));
    n.saturating_sub(1)
}

fn shown(kind: &str, value: &str, style: usize, book: &Book) -> String {
    match kind {
        // Cut before it is copied: one shared string referenced by every cell is the amplifier.
        "s" => value.trim().parse::<usize>().ok().and_then(|i| book.strings.get(i))
            .map(|s| s.chars().take(crate::sheet::MAX_CELL_CHARS + 1).collect()).unwrap_or_default(),
        "b" => if value.trim() == "1" { "TRUE".into() } else { "FALSE".into() },
        "str" | "inlineStr" | "e" | "d" => value.to_string(),
        _ => match value.trim().parse::<f64>() {
            Ok(v) if book.date_styles.get(style).copied().unwrap_or(false) => serial_date(v, book.date1904),
            Ok(v) => number(v),
            Err(_) => value.to_string(),
        },
    }
}

// Excel shows fifteen significant digits, so 0.1 + 0.2 stored as 0.30000000000000004 reads 0.3.
pub fn number(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        return format!("{}", v as i64);
    }
    let rounded: f64 = format!("{:.14e}", v).parse().unwrap_or(v);
    format!("{}", rounded)
}

// A serial date as ISO 8601: whole days from the epoch, and the fraction as the time of day. The
// 1900 system counts a 29 February 1900 that never was, so serials before it start a day later.
pub fn serial_date(serial: f64, date1904: bool) -> String {
    if !serial.is_finite() || serial < 0.0 || serial > 2_958_466.0 {
        return number(serial);
    }
    let mut days = serial.floor() as i64;
    let mut secs = ((serial - serial.floor()) * 86_400.0).round() as i64;
    if secs >= 86_400 {
        days += 1;
        secs -= 86_400;
    }
    let time = if secs % 60 == 0 {
        format!("{:02}:{:02}", secs / 3600, secs / 60 % 60)
    } else {
        format!("{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
    };
    if days == 0 && !date1904 {
        return time;
    }
    let base = if date1904 { days_from_civil(1904, 1, 1) } else if days < 60 { days_from_civil(1899, 12, 31) } else { days_from_civil(1899, 12, 30) };
    let (y, m, d) = civil_from_days(base + days);
    if secs == 0 { format!("{:04}-{:02}-{:02}", y, m, d) } else { format!("{:04}-{:02}-{:02} {}", y, m, d, time) }
}

// Howard Hinnant's days-from-civil and its inverse, over the proleptic Gregorian calendar.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { yoe + era * 400 + 1 } else { yoe + era * 400 }, m, d)
}
