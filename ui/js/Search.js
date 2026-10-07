.pragma library

.import "DirSizes.js" as DirSizes
.import "Format.js" as Format
.import "Thumbs.js" as Thumbs

// The subtree search's behaviour, taking ui/Pane.qml's root the way Focus.js does; Pane holds the
// state, this holds what the state does. The wire is docs/protocol.md "search".
var OFF = ""
var TYPING = "typing"
var RESULTS = "results"

// How long the query line waits after the last keystroke before it walks. A new walk ends the one
// before it in the backend, so typing faster than this costs one walk, not one per character.
var LIVE_MS = 120

// Ctrl+F opens the query line over the open folder alone, Ctrl+Shift+F over it and every folder
// below it. Results follow the query as it is typed; enter hands the keyboard to them.
function start(root, deep) {
    var depth = root.searchDeep !== (deep === true)
    root.searchDeep = deep === true
    root.searchMode = TYPING
    if (depth) schedule(root)
}

// Whether the listing on screen is a walk's rather than the directory's: always after enter, and
// while typing once the first live walk has gone out. searchFrom is set by walk alone.
function listed(root) {
    return root.searchMode === RESULTS || (root.searchMode === TYPING && (root.searchFrom || "").length > 0)
}

function typed(root, character) {
    root.searchQuery += character
    schedule(root)
}

function backspace(root) {
    root.searchQuery = root.searchQuery.substring(0, root.searchQuery.length - 1)
    schedule(root)
}

// ui/Pane.qml's debounce; the pure suites have none, and walk only on enter there.
function schedule(root) {
    if (root.searchLive) root.searchLive.restart()
}

// The debounce fired: walk the query as it stands, or with the line emptied put the folder back.
function live(root) {
    if (root.searchMode !== TYPING) return
    // A re-list still out owns the rows; walking over it would interleave two listings.
    if (root.listInFlight) {
        schedule(root)
        return
    }
    if (root.searchQuery.length === 0) {
        if (listed(root)) {
            var back = root.searchFrom
            reset(root)
            root.openWithoutHistory(back)
        }
        return
    }
    if (root.searchWalked !== walkKey(root)) walk(root)
}

function walkKey(root) {
    return (root.searchDeep ? "deep:" : "here:") + root.searchQuery
}

// Enter commits: the keyboard goes back to the results, so j/k move again. A live walk of this
// exact query is already on screen, or still running, so only a query the debounce never saw walks.
function run(root) {
    if (root.searchLive) root.searchLive.stop()
    if (root.searchQuery.length === 0) {
        close(root)
        return
    }
    // A re-list still out would have its listed taken for the walk's; the debounce retries once it lands.
    if (root.listInFlight) {
        schedule(root)
        return
    }
    if (!listed(root) || root.searchWalked !== walkKey(root)) walk(root)
    root.searchMode = RESULTS
}

function walk(root) {
    // The walk is always the folder the pane is standing in, never home and never root: the
    // operator's ruling of 2026-09-23. The pane's path stays the listing's base, so every result
    // name is relative to it and join, reveal and every per-row facility keep working untouched.
    if ((root.searchFrom || "").length === 0) {
        root.searchFrom = root.path
    }
    root.searchWalked = walkKey(root)
    // Every line the previous walk or window still has on the wire lands before this walk's listed;
    // ui/PaneWire.qml drops them until it arrives, so a superseded walk never paints over this one.
    // A count rather than a flag, because each walk answers exactly one listed and two can be out.
    root.searchPending = (root.searchPending || 0) + 1
    root.searchRunning = true
    root.searchScanned = 0
    root.searchCancelled = false
    root.total = 0
    root.held = 0
    root.rows = []
    root.kindNames = []
    root.cursorIndex = 0
    root.listingState = "loading"
    root.clearSelection()
    root.backend.search(root.path, root.searchQuery, root.showHidden, !root.searchDeep)
}

// Esc stops a running walk and leaves the results up; a second Esc is what returns to the listing.
function cancel(root) {
    if (root.searchRunning) {
        root.backend.searchcancel()
        return
    }
    close(root)
}

// Leaving search re-lists the directory the search was started from. No history entry, because
// entering and leaving a search is not a navigation.
function close(root) {
    var relist = listed(root)
    var back = (root.searchFrom || "").length > 0 ? root.searchFrom : root.path
    if (root.searchLive) root.searchLive.stop()
    root.searchMode = OFF
    root.searchQuery = ""
    reset(root)
    if (relist) {
        root.openWithoutHistory(back)
    }
}

// The walk's own state, which the query line outlives when it is emptied. searchPending is not
// reset: the listed lines it counts are still on the wire and must still be swallowed.
function reset(root) {
    root.searchRunning = false
    root.searchScanned = 0
    root.searchFrom = ""
    root.searchWalked = ""
}

// The query line's own keys while it has the caret, the twin of ui/js/Filter.js typeKey: printable
// characters extend it, backspace shortens it, enter commits the walk and escape abandons it.
// Nothing else reaches the list while the caret is up, so every key answers consumed.
function typeKey(event, root) {
    if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
        run(root)
        return true
    }
    if (event.key === Qt.Key_Escape) {
        close(root)
        return true
    }
    if (event.key === Qt.Key_Backspace) {
        backspace(root)
        return true
    }
    // Tab flips the depth the chord chose, between this folder alone and this folder with every one
    // below it; ui/SearchStrip.qml draws "in <scope>" beside the caret, so every search says which.
    if (event.key === Qt.Key_Tab || event.key === Qt.Key_Backtab) {
        root.searchDeep = !root.searchDeep
        schedule(root)
        return true
    }
    if (event.text.length === 1 && event.text >= " ") {
        typed(root, event.text)
        return true
    }
    return true
}

// o on a result opens the directory that holds it and puts the cursor on the row, the design's reveal.
function reveal(root) {
    var row = root.rowFor(root.cursorIndex)
    if (!row) {
        return
    }
    var full = root.join(root.path, row.n)
    var cut = full.lastIndexOf("/")
    if (cut <= 0) {
        return
    }
    if (root.searchLive) root.searchLive.stop()
    root.searchMode = OFF
    root.searchQuery = ""
    reset(root)
    root.pendingSelect = full
    root.open(full.substring(0, cut))
}

// What activating a row means right now. On a search result the operator's ruling is that it takes
// you to the file instead of opening it; the canvas keeps enter on open and o on reveal, so it is
// only the pointer that asks this. ui/js/Tap.js is the one caller.
function activateAction(root) {
    return listed(root) ? "reveal" : "open"
}

// The terminal searched line: the walk ranks its rows in the statement before writing it, so every
// row index the client still holds names another file, exactly as a re-sort's do. docs/protocol.md
// "searched" states the re-read this discharges; the five moves are ui/js/Sort.js resort's own.
function ranked(root) {
    root.thumbState = Thumbs.empty()
    root.dirSizeState = DirSizes.empty()
    root.clearSelection()
    // Row 0 is the best match once the rank has run, so the reset lands the cursor on the answer.
    root.setCursor(0)
    root.backend.window(0, root.windowSize)
}

// A walk with no matches yet is still working, so the list area keeps the crawl rather than
// flashing the empty state at every directory that happens to hold nothing.
function listingState(root, total) {
    if (total > 0) {
        return "ready"
    }
    return root.searchRunning ? "loading" : "empty"
}

// The strip's right edge: the running count while there is one, the terminal word when there is not.
// SearchFilter rule 6: the count and the state that changes it read together, on the one line the
// query is on, because a count still growing says something different from a count that has settled.
function note(total, running, cancelled) {
    if (total > 0) {
        return running ? total + " found · still scanning" : total + " found"
    }
    if (running) {
        return "searching"
    }
    return cancelled ? "stopped" : "done"
}

// The strip's own right edge, SearchFilter's proposed drawing: while the walk runs esc stops it and
// a second esc is what leaves, which is the pair the board prints as one line.
function wayOut(running) {
    return running ? "esc cancels, then returns" : "esc returns"
}

// The rule itself lives in Format.tilde, because the window chrome draws a path through the same one.
function scope(path, home, deep) {
    return Format.tilde(path, home) + (deep === true ? " and subfolders" : "")
}

// The status bar's own left half while a search is up, the two lines the canvas draws.
function statusLine(running, total, scanned, ms) {
    var found = Format.count(total) + " found"
    if (running) {
        return found + " · Searching, " + Format.count(scanned) + " scanned"
    }
    return found + " in " + (ms / 1000).toFixed(1) + " s"
}
