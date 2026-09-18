.pragma library

.import "Filter.js" as Filter
.import "Keymap.js" as Keymap

// Marking rows: ctrl+A, the shift range in both its keyboard and its pointer form, and ctrl+click.
// Split out of ui/js/Filter.js, which narrows the listing; these move the cursor and the marks over
// whatever that narrowing left drawn, which is why every one of them reads through it.

// Space held down paints: the press marks the row under the cursor and every row the cursor reaches
// before the release takes the SAME state, 1 marking and -1 unmarking, so a held sweep clears a run
// as well as marks one. The operator asked for it on 2026-09-18, "allow me to keep space pressed",
// in place of a press per row. A held key's auto-repeat is swallowed by ui/js/Focus.js at the press,
// or the row under a resting cursor would flip on and off while nothing moved; the paint itself runs
// from ui/Pane.qml's cursor change, the one place every move lands, the grid's own arrows included.
var painting = 0

function paintPress(pane) {
    pane.toggleSelect()
    painting = pane.selection.has(pane.cursorIndex) ? 1 : -1
}

function paintCursor(pane) {
    if (painting !== 0 && pane.selection.has(pane.cursorIndex) !== (painting === 1)) pane.toggleSelect()
}

// Only the painting key's own release ends the sweep, and it is resolved through the same table the
// press was: the arrows are lifted and pressed again throughout a sweep, so ending on any release
// at all painted the first row it reached and then nothing. Which key paints is keys.toml's answer,
// never Qt.Key_Space written down a second time here.
function released(event) {
    if (!event.isAutoRepeat && Keymap.lookup(event.key, event.text, event.modifiers) === "toggleSelect")
        painting = 0
}

// Ctrl+A takes what is drawn, never what is listed: under a filter that is the matches alone.
function selectAll(pane) {
    if (pane.shown === null) {
        pane.selection.all(pane.total)
        return
    }
    pane.selection.clear()
    for (var i = 0; i < pane.shown.length; i++) {
        pane.selection.toggle(pane.shown[i])
    }
}

// Shift+J and Shift+K, the whole gesture: the cursor moves through what is drawn and the selection
// follows it. corner: the anchor only re-latches to the cursor once the selection is empty, so a
// plain j/k move never has to special-case a shift+j/k chain already in progress.
function extend(pane, delta) {
    if (pane.selection.count() === 0) {
        pane.selectionAnchor = pane.cursorIndex
    }
    Filter.moveCursor(pane, delta)
    extendTo(pane, pane.selectionAnchor)
    pane.selectionVersion += 1
}

// Shift+click, the absolute twin of extend() above: the cursor lands on the clicked row and the
// selection covers the drawn rows between it and the anchor the gesture started from.
function extendToRow(pane, index) {
    if (pane.selection.count() === 0) {
        pane.selectionAnchor = pane.cursorIndex
    }
    Filter.setCursor(pane, index)
    extendTo(pane, pane.selectionAnchor)
    pane.selectionVersion += 1
}

// Ctrl+click, v's mouse twin. An empty set means the cursor row is selected, so it joins first.
function toggleRow(pane, index) {
    if (pane.selection.count() === 0 && pane.cursorIndex !== index) {
        pane.selection.toggle(pane.cursorIndex)
    }
    Filter.setCursor(pane, index)
    pane.selection.toggle(index)
    pane.selectionAnchor = index
    pane.selectionVersion += 1
}

// The rows drawn between the cursor and the anchor, for both gestures above.
function extendTo(pane, anchor) {
    if (pane.shown === null) {
        pane.selection.extendTo(pane.cursorIndex, anchor)
        return
    }
    pane.selection.clear()
    var range = Filter.between(pane.shown, pane.cursorIndex, anchor)
    for (var i = 0; i < range.length; i++) {
        pane.selection.toggle(range[i])
    }
}
