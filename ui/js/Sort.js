.pragma library

.import "DirSizes.js" as DirSizes
.import "Thumbs.js" as Thumbs

// What the header's click and the s and S keys do, taking ui/Pane.qml's root the way Nav.js and
// Ops.js do: the pane holds the state, this holds what the state does. ui/Backend.qml records the
// order the listing is actually in, because list re-sorts by name ascending and only this file
// changes it after that.

// The orders the backend can actually produce, in the order s steps through them, and the only keys
// that may move the recorded order. It is not the list of what gets refused: docs/protocol.md "sort"
// refuses every other key by name, and the backend is the one that says so, see ui/js/Errors.js.
var ORDERS = ["name", "size", "mtime", "kind"]

// Ctrl+J, Ctrl+K and Ctrl+L, the operator's ruling of 2026-09-23: one order each, starting in the
// direction that order is wanted first, A to Z, biggest first and latest first. The chord of the
// order already shown reverses it; a chord for another order always starts at that one's default.
var CHORDS = { sortName: "name", sortSize: "size", sortDate: "mtime" }
var FIRST_DESC = { name: false, size: true, mtime: true }

function chord(pane, action) {
    var key = CHORDS[action]
    resort(pane, key, pane.backend.sortBy === key ? !pane.backend.sortDesc : FIRST_DESC[key])
}

// The keyboard's one way in, so ui/js/Focus.js carries a single case for every sort key.
function byKey(pane, action) {
    if (action === "sortNext") next(pane)
    else if (action === "sortReverse") reverse(pane)
    else chord(pane, action)
}

// ui/Header.qml's click. The column already sorted reverses; any other column starts ascending,
// which is the order the canvas's own header draws beside "Name". Only ORDERS may leave this file.
function column(pane, key) {
    // Unsupported columns remain labels rather than sending a sort the backend must refuse.
    if (ORDERS.indexOf(key) < 0)
        return
    resort(pane, key, pane.backend.sortBy === key ? !pane.backend.sortDesc : false)
}

// s: the next order the backend can produce, always ascending, because the column and the direction
// are separate choices. It walks ORDERS, so it never lands on a column that would only earn a
// refusal; an aimed click on one of those earns the reason, a key that walks onto it earns noise.
function next(pane) {
    var at = ORDERS.indexOf(pane.backend.sortBy)
    resort(pane, ORDERS[(at + 1) % ORDERS.length], false)
}

// S: reverse whichever order the listing is in, the capital-is-the-variant pair g/G and j/J use.
function reverse(pane) {
    resort(pane, pane.backend.sortBy, !pane.backend.sortDesc)
}

// The request goes out for every key, so the refusal is the backend's alone. Only an order it will
// really produce moves the recorded one, or the mark would describe a listing that never changed.
function resort(pane, key, desc) {
    // Asking for the order the listing is already in would drop every row-indexed cache and put the
    // cursor back to redraw the rows already on screen, so it is not asked for at all.
    if (pane.backend.sortBy === key && pane.backend.sortDesc === desc) {
        return
    }
    pane.backend.sort(key, desc)
    if (ORDERS.indexOf(key) < 0) {
        return
    }
    pane.backend.sortBy = key
    pane.backend.sortDesc = desc
    // Held for the session and never written to ui.json, so every launch starts at the saved default.
    pane.backend.sessionSort = { key: key, reverse: desc }
    // A reorder moves every row, so the caches keyed by a row index are as stale as a new listing's,
    // and a selection of row indices would silently come to name different files.
    pane.thumbState = Thumbs.empty()
    pane.dirSizeState = DirSizes.empty()
    pane.clearSelection()
    pane.setCursor(0)
    // sort emits no rows of its own, so the reordered window is asked for here; see docs/protocol.md.
    pane.backend.window(0, pane.windowSize)
}
