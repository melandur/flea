// An OpenDocument spreadsheet's first table as text: what each cell displays, which ODF stores
// beside the typed value, so a date reads the way LibreOffice shows it. The same content serves a
// packaged .ods (its content.xml) and a flat .fods, which is that document as one file.
use crate::sheet::{clip, Limits};
use crate::sheetxml::{attr, decode, tokens, Tok};

// ODF compresses runs of identical cells and rows into one element with a repeat count, and a
// saved sheet ends every row with ~1000 empty columns and the table with ~1M empty rows. Empties
// are therefore held back and only written once something real follows them.
pub fn rows(content: &str, limits: Limits) -> Vec<Vec<String>> {
    let (limit, max_cols) = (limits.rows, limits.columns);
    let mut spent = 0usize;
    let mut out: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let (mut depth, mut row_repeat, mut pending_rows) = (0usize, 1usize, 0usize);
    let (mut in_cell, mut cell_repeat, mut pending_cells) = (false, 1usize, 0usize);
    let (mut text, mut value, mut paragraphs, mut para, mut note) = (String::new(), String::new(), 0usize, 0usize, 0usize);
    for t in tokens(content) {
        if out.len() >= limit || spent >= limits.bytes {
            break;
        }
        match t {
            Tok::Open { name: "table", empty: false, .. } => depth += 1,
            Tok::Close("table") if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ if depth == 0 => {}
            Tok::Open { name: "table-row", attrs, empty } => {
                row_repeat = repeat(attrs, "number-rows-repeated");
                row.clear();
                pending_cells = 0;
                if empty {
                    pending_rows = pending_rows.saturating_add(row_repeat);
                }
            }
            Tok::Close("table-row") => {
                if row.is_empty() {
                    pending_rows = pending_rows.saturating_add(row_repeat);
                    continue;
                }
                while pending_rows > 0 && out.len() < limit {
                    out.push(Vec::new());
                    pending_rows -= 1;
                }
                let width: usize = row.iter().map(|c| c.len() + 1).sum();
                for _ in 0..row_repeat.min(limit.saturating_sub(out.len())) {
                    if spent >= limits.bytes {
                        break;
                    }
                    spent += width;
                    out.push(row.clone());
                }
            }
            Tok::Open { name: "table-cell" | "covered-table-cell", attrs, empty } => {
                cell_repeat = repeat(attrs, "number-columns-repeated");
                text.clear();
                paragraphs = 0;
                para = 0;
                value = ["value", "date-value", "time-value", "boolean-value", "string-value"].iter()
                    .find_map(|k| attr(attrs, k)).unwrap_or_default();
                in_cell = !empty;
                if empty {
                    pending_cells = pending_cells.saturating_add(cell_repeat);
                }
            }
            Tok::Close("table-cell" | "covered-table-cell") if in_cell => {
                in_cell = false;
                let shown = clip(if paragraphs == 0 && text.is_empty() { std::mem::take(&mut value) } else { std::mem::take(&mut text) }, limits.cell_chars);
                if shown.is_empty() {
                    pending_cells = pending_cells.saturating_add(cell_repeat);
                    continue;
                }
                while pending_cells > 0 && row.len() < max_cols {
                    row.push(String::new());
                    pending_cells -= 1;
                }
                pending_cells = 0;
                // A row's own width is the budget too, before its repeats multiply it.
                for _ in 0..cell_repeat.min(max_cols.saturating_sub(row.len())) {
                    if row.iter().map(String::len).sum::<usize>() >= limits.bytes {
                        break;
                    }
                    row.push(shown.clone());
                }
            }
            // A comment on a cell is not what the cell shows.
            Tok::Open { name: "annotation", empty: false, .. } if in_cell => note += 1,
            Tok::Close("annotation") if note > 0 => note -= 1,
            _ if !in_cell || note > 0 => {}
            // Only a paragraph's own text shows: a pretty-printed .fods indents between the elements.
            Tok::Open { name: "p" | "h", empty, .. } => {
                if paragraphs > 0 {
                    text.push('\n');
                }
                paragraphs += 1;
                if !empty {
                    para += 1;
                }
            }
            Tok::Close("p" | "h") if para > 0 => para -= 1,
            _ if para == 0 => {}
            Tok::Open { name: "s", attrs, .. } => {
                let n = attr(attrs, "c").and_then(|v| v.parse::<usize>().ok()).unwrap_or(1).min(256);
                text.extend(std::iter::repeat(' ').take(n));
            }
            Tok::Open { name: "tab", .. } => text.push('\t'),
            Tok::Open { name: "line-break", .. } => text.push('\n'),
            _ if text.len() >= limits.bytes => {}
            Tok::Text(s) => text.push_str(&decode(s)),
            Tok::Cdata(s) => text.push_str(s),
            _ => {}
        }
    }
    out
}

fn repeat(attrs: &str, key: &str) -> usize {
    attr(attrs, key).and_then(|v| v.parse().ok()).filter(|&n: &usize| n > 0).unwrap_or(1)
}
