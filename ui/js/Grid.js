.pragma library

.import "Filter.js" as Filter
.import "Keymap.js" as Keymap

// The grid's own geometry: which cell a key means when the rows are tiles rather than lines. Split
// out of ui/js/Focus.js, which holds the key map and the actions every view shares.

// Whether the preset in force spells the arrows as letters at all. Asked of the map rather than
// named preset by preset, so a preset that gives h back gets the grid back with it; Default's own
// suppressing rows answer "" here, and the four hardcoded letters below read the same test.
function spelled() {
    return Keymap.lookup(0, "h", 0, "listing") !== ""
}

// Which way a key steps across a row of tiles: the arrow always, and the letter wherever it means
// anything, so issue 114 goes on holding and the grid stops answering where nothing else does.
function sideways(event) {
    var letters = spelled()
    return event.key === Qt.Key_Left || (letters && event.text === "h") ? -1
         : event.key === Qt.Key_Right || (letters && event.text === "l") ? 1 : 0
}

// An arrow requires an existing visual cell, even when item-order navigation wraps at the ends.
function arrow(event, action, root) {
    if (root.viewMode !== "grid") return false
    var columns = root.cursorStride
    var across = sideways(event)
    var delta = event.key === Qt.Key_Down && (action === "cursorDown" || action === "extendDown") ? columns
              : event.key === Qt.Key_Up && (action === "cursorUp" || action === "extendUp") ? -columns
              : across < 0 && action === "cursorLeft" ? -1
              : across > 0 && action === "cursorRight" ? 1 : 0
    if (!delta) return false
    var index = Filter.viewOf(root.shown, root.cursorIndex)
    var nextIndex = index + delta
    if (nextIndex < 0 || nextIndex >= root.shownTotal
            || (across < 0 && index % columns === 0)
            || (across > 0 && nextIndex % columns === 0)) return true
    if (action === "extendDown" || action === "extendUp") root.extendSelection(delta)
    else Filter.moveCursor(root, delta)
    return true
}
