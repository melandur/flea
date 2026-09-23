.pragma library

// A delimited text body, CSV and its cousins, read as records and fields for ui/PreviewTable.qml.
// RFC 4180 as it is written in practice: a quote only opens at the start of a field, so the inch
// mark in 5" screen stays text, and a quoted field keeps its separators and its line breaks, which
// is why a record is never a line and the body is walked quote by quote rather than split on \n.

// The separators a .csv is sniffed between, in the order a tie goes to: Excel writes ; where the
// locale's decimal mark is a comma, so the extension alone cannot say which one a file holds.
var CANDIDATES = [",", ";", "\t", "|"]
// How much of the body the sniff and the column widths read: enough records to be sure, and never
// the whole of a large file just to size a column.
var SAMPLE_RECORDS = 200
var SAMPLE_CHARS = 65536

var FIXED = { ".tsv": "\t", ".tab": "\t", ".psv": "|" }

// The separator a name promises, or "" when the body has to say: a .csv is the only sniffed one.
function hint(name) {
    var lower = String(name).toLowerCase()
    var dot = lower.lastIndexOf(".")
    var ext = dot > 0 ? lower.substring(dot) : ""
    return FIXED[ext] || ""
}

// Where the first record starts: past a UTF-8 byte order mark, which FileView hands over as U+FEFF
// and which would otherwise be the first character of the first header.
function bodyStart(text) {
    return text.charCodeAt(0) === 0xFEFF ? 1 : 0
}

// A quote at q opens a quoted field only where a field starts: the body's first character, or
// straight after a separator or a line break.
function opensField(text, q, delim, recordStart) {
    if (q === recordStart) return true
    var before = text.charAt(q - 1)
    return before === delim || before === "\n" || before === "\r"
}

// The offset each record starts at, up to limit records, with the body's end pushed last so record i
// is always starts[i] to starts[i + 1]. indexOf does the walking, so a body with no quotes costs one
// native search per record.
function recordStarts(text, delim, limit) {
    var starts = []
    var n = text.length
    var pos = bodyStart(text)
    while (pos < n && starts.length < limit) {
        starts.push(pos)
        var from = pos
        for (;;) {
            var nl = text.indexOf("\n", from)
            var q = text.indexOf("\"", from)
            if (q < 0 || (nl >= 0 && nl < q)) { pos = nl < 0 ? n : nl + 1; break }
            if (!opensField(text, q, delim, starts[starts.length - 1])) { from = q + 1; continue }
            // A doubled quote is a quote inside the field, not its end.
            var close = text.indexOf("\"", q + 1)
            while (close >= 0 && text.charAt(close + 1) === "\"") close = text.indexOf("\"", close + 2)
            if (close < 0) { pos = n; break }
            from = close + 1
        }
    }
    starts.push(pos < n ? pos : n)
    return starts
}

// One record's fields, quotes removed and doubled quotes made single; the line break that ends the
// record, \n or \r\n, is not part of its last field.
function fields(text, start, end, delim) {
    var out = []
    var cell = ""
    var quoted = false
    var atStart = true
    var stop = end
    if (stop > start && text.charAt(stop - 1) === "\n") stop--
    if (stop > start && text.charAt(stop - 1) === "\r") stop--
    for (var i = start; i < stop; i++) {
        var c = text.charAt(i)
        if (quoted) {
            if (c !== "\"") cell += c
            else if (text.charAt(i + 1) === "\"" && i + 1 < stop) { cell += "\""; i++ }
            else quoted = false
        } else if (c === delim) {
            out.push(cell); cell = ""; atStart = true; continue
        } else if (c === "\"" && atStart) {
            quoted = true
        } else {
            cell += c
        }
        atStart = false
    }
    out.push(cell)
    return out
}

// The separator a body uses: the name's own when it promises one, else the candidate that splits
// the sampled records into the same number of fields most often, more than one field each. Ties go
// to the wider split and then to CANDIDATES order; a body nothing splits falls back to a comma.
function sniff(text, name) {
    var fixed = hint(name)
    if (fixed.length > 0) return fixed
    var sample = text.substring(0, SAMPLE_CHARS)
    var best = ",", bestAgree = 0, bestWidth = 0
    for (var k = 0; k < CANDIDATES.length; k++) {
        var d = CANDIDATES[k]
        var starts = recordStarts(sample, d, SAMPLE_RECORDS)
        var tally = {}
        // A sample cut mid-record leaves a torn last one, which is not evidence either way.
        var usable = starts.length - (sample.length < text.length ? 2 : 1)
        for (var r = 0; r < usable; r++) {
            var width = fields(sample, starts[r], starts[r + 1], d).length
            if (width > 1) tally[width] = (tally[width] || 0) + 1
        }
        for (var w in tally) {
            var agree = tally[w], wide = Number(w)
            if (agree > bestAgree || (agree === bestAgree && wide > bestWidth)) {
                best = d; bestAgree = agree; bestWidth = wide
            }
        }
    }
    return best
}

// The widest field each column holds across the first records, in characters and capped, which is
// what a monospace face turns into a column width; the column count is the widest record's.
function columnChars(text, starts, delim, cap) {
    var chars = []
    var count = Math.min(starts.length - 1, SAMPLE_RECORDS)
    for (var r = 0; r < count; r++) {
        var row = fields(text, starts[r], starts[r + 1], delim)
        for (var c = 0; c < row.length; c++) {
            var len = Math.min(cap, longestLine(row[c]))
            if (chars[c] === undefined || len > chars[c]) chars[c] = len
        }
    }
    return chars
}

// Which columns hold only numbers below the header, sampled like the widths, so the table can align
// them on their last digit the way every spreadsheet does; an empty cell neither proves nor spoils it.
var NUMBER = /^[-+]?(\d{1,3}([ ,.'\u00a0]\d{3})*|\d+)([.,]\d+)?([eE][-+]?\d+)?%?$/

function numericColumns(text, starts, delim) {
    var seen = [], spoiled = []
    var count = Math.min(starts.length - 1, SAMPLE_RECORDS)
    for (var r = 1; r < count; r++) {
        var row = fields(text, starts[r], starts[r + 1], delim)
        for (var c = 0; c < row.length; c++) {
            var cell = row[c].trim()
            if (cell.length === 0) continue
            if (NUMBER.test(cell)) seen[c] = true
            else spoiled[c] = true
        }
    }
    var out = []
    for (var k = 0; k < Math.max(seen.length, spoiled.length); k++) out.push(seen[k] === true && spoiled[k] !== true)
    return out
}

// A quoted field can hold line breaks, and the cell draws only its first line and the mark shown()
// adds, so the rest are not width.
function longestLine(cell) {
    var nl = cell.indexOf("\n")
    return nl < 0 ? cell.length : shown(cell).length
}

// What a cell draws: its first line, marked when more of it is hidden below.
function shown(cell) {
    var nl = cell.indexOf("\n")
    return nl < 0 ? cell : cell.substring(0, nl).replace(/\r$/, "") + " ↵"
}
