.pragma library

// A text preview's body cut into runs of whole lines, so ui/PreviewText.qml lays out only the runs on
// screen: one Text over the whole file wrapped every character of it on open and again on every
// width change, 90 ms for 650 KB measured on 2026-09-23, where a run is a few hundred bytes of work.

// A run closes at the first newline past this many characters.
var TARGET = 4096
// A line with no newline this far past the target is cut anyway, so a minified file cannot make
// one run of the whole body; the cut draws as one more wrap in a line that was wrapping already.
var LIMIT = 16384

// The newline that closes each run is dropped, because the run's own box ends the line; a blank line
// straddling a boundary survives as the next run's leading newline.
function split(text) {
    var runs = []
    var start = 0
    while (start < text.length) {
        var end = text.indexOf("\n", start + TARGET)
        if (end < 0 && text.length - start <= LIMIT) end = text.length
        else if (end < 0 || end - start > LIMIT) {
            // Past the limit, the last line break before the target still beats a cut mid-line.
            var back = text.lastIndexOf("\n", start + TARGET)
            end = back > start ? back : start + LIMIT
        }
        var cut = text.charAt(end) === "\n" ? end + 1 : end
        runs.push(text.substring(start, end))
        start = cut
    }
    return runs
}
