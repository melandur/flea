.pragma library

.import "Keymap.js" as Keymap
.import "QueryLine.js" as QueryLine
.import "Search.js" as Search

// What the keyboard does to reach the search without its chord, split out of Search.js at its cap
// the way PreviewKeys.js and RailKeys.js came out of Focus.js: typing straight into a folder, typing
// back onto the line from the results, and Right leaving the line. Search.js holds what they do.

// A key that leaves the line commits it, but Right then browses into row 0, and until the walk of
// this exact query has ranked its rows that is no row at all or merely the first one found.
function commitFor(root, action) {
    if (action === "pageForward" && root.searchQuery.length > 0
        && (root.searchRunning || root.searchWalked !== Search.walkKey(root) || !Search.listed(root))) {
        root.message("Still searching; Right opens a result once the search is done.", false)
        return false
    }
    Search.run(root)
    return true
}

// Typing straight into the folder, the operator's ruling of 2026-10-28: a printable key no binding
// claims opens the query line with that character in it, and on the results Backspace and every
// printable key the listing does not bind go back to the line, so typing and the arrows interleave;
// Space, m, comma and ? keep their meaning there, so a result can still be marked and given a menu.
function typeAhead(event, root) {
    if (root.searchMode !== Search.OFF || root.shown !== null || !QueryLine.printable(event)) return false
    Search.start(root, false)
    Search.typed(root, event.text)
    return true
}

function resumeKey(event, root) {
    if (root.searchMode !== Search.RESULTS || root.focusView !== "list") return false
    if (event.key !== Qt.Key_Backspace && (!QueryLine.printable(event)
        || Keymap.lookup(event.key, event.text, event.modifiers, "listing") !== "")) return false
    root.searchMode = Search.TYPING
    if (event.key === Qt.Key_Backspace) Search.backspace(root)
    else Search.typed(root, event.text)
    return true
}
