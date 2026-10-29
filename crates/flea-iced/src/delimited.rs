// ui/js/Delimited.js: one reader for .csv, .tsv, .tab and .psv, and for the CSV `flea --sheet` writes.
// A quote opens a field only at its start, a doubled quote inside one is a quote, and a newline inside
// one belongs to the cell rather than ending the record. Indices are char positions, as JavaScript's
// string indices are UTF-16 ones; every boundary this reads is ASCII, so the two never disagree.

const CANDIDATES: [char; 4] = [',', ';', '\t', '|'];
pub const SAMPLE_RECORDS: usize = 200;
const SAMPLE_CHARS: usize = 65536;
const MAX_RECORD_CHARS: usize = 65536;

fn hint(name: &str) -> Option<char> {
    let lower = name.to_lowercase();
    let ext = lower.rfind('.').filter(|&d| d > 0).map(|d| &lower[d..]).unwrap_or("");
    match ext {
        ".tsv" | ".tab" => Some('\t'),
        ".psv" => Some('|'),
        _ => None,
    }
}

fn body_start(text: &[char]) -> usize {
    usize::from(text.first() == Some(&'\u{FEFF}'))
}

fn find(text: &[char], c: char, from: usize) -> Option<usize> {
    text.get(from..)?.iter().position(|&x| x == c).map(|i| i + from)
}

fn opens_field(text: &[char], q: usize, delim: char, record_start: usize) -> bool {
    q == record_start || matches!(text[q - 1], c if c == delim || c == '\n' || c == '\r')
}

// The char index each record starts at, up to `limit` records, then one past the last one read.
pub fn record_starts(text: &[char], delim: char, limit: usize) -> Vec<usize> {
    let n = text.len();
    let mut starts = vec![];
    let mut pos = body_start(text);
    while pos < n && starts.len() < limit {
        starts.push(pos);
        let mut from = pos;
        loop {
            let nl = find(text, '\n', from);
            let q = find(text, '"', from);
            match (q, nl) {
                (None, _) => {
                    pos = nl.map(|i| i + 1).unwrap_or(n);
                    break;
                }
                (Some(q), Some(nl)) if nl < q => {
                    pos = nl + 1;
                    let _ = q;
                    break;
                }
                (Some(q), _) => {
                    if !opens_field(text, q, delim, *starts.last().unwrap()) {
                        from = q + 1;
                        continue;
                    }
                    let mut close = find(text, '"', q + 1);
                    while let Some(c) = close.filter(|&c| text.get(c + 1) == Some(&'"')) {
                        close = find(text, '"', c + 2);
                    }
                    match close {
                        None => {
                            pos = n;
                            break;
                        }
                        Some(c) => from = c + 1,
                    }
                }
            }
        }
    }
    starts.push(pos.min(n));
    starts
}

pub fn fields(text: &[char], start: usize, end: usize, delim: char) -> Vec<String> {
    let mut out = vec![];
    let mut cell = String::new();
    let (mut quoted, mut at_start) = (false, true);
    let mut stop = end;
    if stop > start && text[stop - 1] == '\n' {
        stop -= 1;
    }
    if stop > start && text[stop - 1] == '\r' {
        stop -= 1;
    }
    stop = stop.min(start + MAX_RECORD_CHARS);
    let mut i = start;
    while i < stop {
        let c = text[i];
        if quoted {
            if c != '"' {
                cell.push(c);
            } else if i + 1 < stop && text[i + 1] == '"' {
                cell.push('"');
                i += 1;
            } else {
                quoted = false;
            }
        } else if c == delim {
            out.push(std::mem::take(&mut cell));
            at_start = true;
            i += 1;
            continue;
        } else if c == '"' && at_start {
            quoted = true;
        } else {
            cell.push(c);
        }
        at_start = false;
        i += 1;
    }
    out.push(cell);
    out
}

// Delimited.sniff: the delimiter most sampled records agree on a width above one for, the wider
// width winning a tie; a fixed extension decides without sampling.
pub fn sniff(text: &[char], name: &str) -> char {
    if let Some(d) = hint(name) {
        return d;
    }
    let sample = &text[..text.len().min(SAMPLE_CHARS)];
    let (mut best, mut best_agree, mut best_width) = (',', 0, 0);
    for d in CANDIDATES {
        let starts = record_starts(sample, d, SAMPLE_RECORDS);
        // A sample cut short of the text drops its last record, which the cut may have broken.
        let usable = starts.len().saturating_sub(if sample.len() < text.len() { 2 } else { 1 });
        let mut tally: std::collections::BTreeMap<usize, usize> = Default::default();
        for r in 0..usable {
            let width = fields(sample, starts[r], starts[r + 1], d).len();
            if width > 1 {
                *tally.entry(width).or_default() += 1;
            }
        }
        for (width, agree) in tally {
            if agree > best_agree || (agree == best_agree && width > best_width) {
                (best, best_agree, best_width) = (d, agree, width);
            }
        }
    }
    best
}

// What a cell shows on one line: up to its first newline, then a return mark.
pub fn shown(cell: &str) -> String {
    match cell.find('\n') {
        None => cell.to_string(),
        Some(nl) => format!("{} ↵", cell[..nl].strip_suffix('\r').unwrap_or(&cell[..nl])),
    }
}

pub fn column_chars(text: &[char], starts: &[usize], delim: char, cap: usize) -> Vec<usize> {
    let mut chars: Vec<usize> = vec![];
    for r in 0..starts.len().saturating_sub(1).min(SAMPLE_RECORDS) {
        for (c, cell) in fields(text, starts[r], starts[r + 1], delim).iter().enumerate() {
            let len = shown(cell).chars().count().min(cap);
            if c >= chars.len() {
                chars.push(len);
            } else if len > chars[c] {
                chars[c] = len;
            }
        }
    }
    chars
}

// Delimited.NUMBER, by hand: an optional sign, digits grouped by three with a space, comma, dot,
// apostrophe or no-break space or plain, an optional decimal part, exponent and percent sign.
fn is_number(s: &str) -> bool {
    let s = s.strip_suffix('%').unwrap_or(s);
    let s = s.strip_prefix(['-', '+']).unwrap_or(s);
    let (mantissa, exp) = match s.find(['e', 'E']) {
        Some(i) => (&s[..i], Some(&s[i + 1..])),
        None => (s, None),
    };
    if let Some(e) = exp {
        let e = e.strip_prefix(['-', '+']).unwrap_or(e);
        if e.is_empty() || !e.bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
    }
    let digits = |p: &str| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit());
    // (\d{1,3}([ ,.'\u00a0]\d{3})*|\d+): plain digits, or one to three then groups of three,
    // each group led by its own separator, so a mix of them is still one number.
    let integer = |p: &str| -> bool {
        if digits(p) {
            return true;
        }
        let chars: Vec<char> = p.chars().collect();
        let lead = chars.iter().take_while(|c| c.is_ascii_digit()).count();
        if !(1..=3).contains(&lead) {
            return false;
        }
        let mut i = lead;
        while i < chars.len() {
            if ![' ', ',', '.', '\'', '\u{a0}'].contains(&chars[i]) || i + 4 > chars.len() || !chars[i + 1..i + 4].iter().all(char::is_ascii_digit) {
                return false;
            }
            i += 4;
        }
        true
    };
    if integer(mantissa) {
        return true;
    }
    // ([.,]\d+)?: the decimal part is all digits, so only the last [.,] can be its point.
    match mantissa.rfind(['.', ',']) {
        Some(i) => digits(&mantissa[i + 1..]) && integer(&mantissa[..i]),
        None => false,
    }
}

pub fn numeric_columns(text: &[char], starts: &[usize], delim: char) -> Vec<bool> {
    let (mut seen, mut spoiled): (Vec<bool>, Vec<bool>) = (vec![], vec![]);
    for r in 1..starts.len().saturating_sub(1).min(SAMPLE_RECORDS) {
        for (c, cell) in fields(text, starts[r], starts[r + 1], delim).iter().enumerate() {
            let cell = cell.trim();
            if cell.is_empty() {
                continue;
            }
            let slot = if is_number(cell) { &mut seen } else { &mut spoiled };
            if slot.len() <= c {
                slot.resize(c + 1, false);
            }
            slot[c] = true;
        }
    }
    (0..seen.len().max(spoiled.len())).map(|k| seen.get(k) == Some(&true) && spoiled.get(k) != Some(&true)).collect()
}

#[cfg(test)]
#[path = "delimited_tests.rs"]
mod tests;
