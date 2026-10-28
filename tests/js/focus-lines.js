.import "../../ui/js/Focus.js" as Focus
.import "filterfixture.js" as Fixture

// Issue 12's own suite, split out of tests/js/focus.js at its 300-line cap the way focus-forward.js
// was: what the search's and the filter's query lines do with a key while they hold the caret.

function closed() {
    return { active: false, isMedia: false, isPdf: false }
}

function key(code, text) {
    return { key: code, text: text, modifiers: Qt.NoModifier }
}

// A pane with the whole handleKey route wired, because issue 12's defect is that a query line
// swallowed the key: only the real route can show a key reaching the rows or failing to.
function queryPane() {
    var p = Fixture.pane()
    p.focusView = "list"
    p.viewMode = "list"
    p.searchMode = ""
    p.searchQuery = ""
    p.searchFrom = ""
    p.searchDeep = false
    p.searchRunning = false
    p.searchScanned = 0
    p.searchCancelled = false
    p.home = ""
    p.path = "/d"
    p.showHidden = false
    p.listingState = "ready"
    p.kindNames = []
    p.trashArmedAt = 0
    p.inputAt = 0
    p.rowsAt = 0
    p.cursorStride = 1
    // Half of this is a page key's step, so pageDown and pageUp move a real distance here.
    p.visibleRows = 4
    p.preview = closed()
    p.shareBrowser = { active: false }
    p.said = ""
    p.walked = []
    p.message = function (text, isError) { p.said = text }
    p.clearSelection = function () { p.picked = {} }
    p.renameEditor = function () { return null }
    p.act = function (action) { Focus.act(action, p) }
    p.backend = { search: function (path, query, hidden, shallow) {
        p.walked.push(path + "?" + query + (shallow ? "" : "+deep"))
    } }
    return p
}

// Focus.handleKey takes the rail only to ask it for a live rename editor.
function noRail() {
    return { renameEditor: function () { return null } }
}

function run(check) {
    // Issue 12: both query lines owned every key while they had the caret, so an arrow never reached
    // the rows and a listing with more than one match could not be walked from the keyboard.
    var narrowing = queryPane()
    narrowing.filterTyping = true
    narrowing.filterQuery = "scr"
    narrowing.refresh()
    Focus.handleKey(key(Qt.Key_Down, ""), narrowing, noRail())
    check("an arrow on the filter's query line hands the caret back to the rows",
          narrowing.filterTyping, false)
    check("and leaves the filter standing, so the narrowed rows are what is moved through",
          narrowing.filterQuery, "scr")
    check("and moves the cursor on that same press, to the second row the filter kept",
          narrowing.cursorIndex, 1)
    // The line is still a line: every printable character narrows it rather than reaching the rows.
    var typingOn = queryPane()
    typingOn.filterTyping = true
    typingOn.filterQuery = "scr"
    typingOn.refresh()
    Focus.handleKey(key(Qt.Key_J, "j"), typingOn, noRail())
    check("and j is still typed into the query rather than moving the cursor",
          typingOn.filterQuery + "|" + typingOn.filterTyping, "scrj|true")

    // The search's line commits the walk the way enter does, so one press walks once: a walk per
    // keystroke is the sweep the design refused, and it is not what an arrow asks for either.
    var walking = queryPane()
    walking.searchMode = "typing"
    walking.searchQuery = "scr"
    Focus.handleKey(key(Qt.Key_Down, ""), walking, noRail())
    check("an arrow on the search's query line commits the walk and leaves the line",
          walking.searchMode + "|" + walking.walked.join(","), "results|/d?scr")
    var stillTyping = queryPane()
    stillTyping.searchMode = "typing"
    stillTyping.searchQuery = "scr"
    Focus.handleKey(key(Qt.Key_E, "e"), stillTyping, noRail())
    check("and a printable key still extends the query without walking anything",
          stillTyping.searchQuery + "|" + stillTyping.walked.length, "scre|0")

    // Issue 28's four keys left the table when it was cut back to the arrows and the Ctrl chords,
    // so the up arrow is the one key here that still carries a listing meaning onto the query line.
    // ui/SearchStrip.qml draws the query as a Text with the caret pinned after it, so there is no
    // caret on the line for it to move: ui/js/Focus.js LEAVES_LINE. "scr" keeps rows 0, 1, 5 and 6,
    // and the cursor starts on the third of the four, so leaving the line is half the behaviour and
    // the key still reaching the listing is the other half.
    var others = [Qt.Key_Up]
    var handed = []
    var landed = []
    var committed = []
    for (var i = 0; i < others.length; i++) {
        var narrowed = queryPane()
        narrowed.filterTyping = true
        narrowed.filterQuery = "scr"
        narrowed.refresh()
        narrowed.cursorIndex = 5
        Focus.handleKey(key(others[i], ""), narrowed, noRail())
        handed.push(narrowed.filterTyping + ":" + narrowed.filterQuery)
        landed.push(narrowed.cursorIndex)
        var walks = queryPane()
        walks.searchMode = "typing"
        walks.searchQuery = "scr"
        Focus.handleKey(key(others[i], ""), walks, noRail())
        committed.push(walks.searchMode + ":" + walks.walked.length)
    }
    check("the up arrow hands the filter's caret back, with its query left standing",
          handed.join("|"), "false:scr")
    check("and then means what it means in the listing, off the row it started on",
          landed.join("|"), "1")
    check("and commits the search's walk exactly once, the way the down arrow does",
          committed.join("|"), "results:1")

    // The operator's ruling of 2026-10-28: Right on the line commits and browses in, so a folder
    // among the results is entered, the search ends there, and nothing is walked a second time.
    var entering = queryPane()
    entering.searchMode = "typing"
    entering.searchQuery = "scr"
    entering.searchFrom = "/d"
    entering.searchWalked = "here:scr"
    entering.openCursor = function () { entering.opened = entering.rowFor(entering.cursorIndex).n }
    Focus.handleKey(key(Qt.Key_Right, ""), entering, noRail())
    check("right on the search's line opens the folder under the cursor without a second walk",
          entering.opened + "|" + entering.walked.length, "Screens|0")
    // Until the walk of this exact query has ranked, row 0 is no row or only the first one found.
    function early(running, walked) {
        var p = queryPane()
        p.searchMode = "typing"
        p.searchQuery = "scr"
        p.searchFrom = "/d"
        p.searchWalked = walked
        p.searchRunning = running
        p.openCursor = function () { p.opened = "yes" }
        Focus.handleKey(key(Qt.Key_Right, ""), p, noRail())
        return (p.opened || "no") + "|" + p.searchMode + "|" + p.walked.length + "|" + p.said
    }
    var held = "no|typing|0|Still searching; Right opens a result once the search is done."
    check("and while that walk still runs right opens nothing and says why", early(true, "here:scr"), held)
    check("nor before the debounce has walked the query typed", early(false, "here:sc"), held)
    var filtered = queryPane()
    filtered.filterTyping = true
    filtered.filterQuery = "scr"
    filtered.refresh()
    filtered.openCursor = function () { filtered.opened = filtered.rowFor(filtered.cursorIndex).n }
    Focus.handleKey(key(Qt.Key_Right, ""), filtered, noRail())
    check("right on the filter's line enters the folder under the cursor the same way",
          filtered.opened + "|" + filtered.filterTyping, "Screens|false")

    // And typing reaches the search with no chord first: an unbound printable key opens the line
    // with itself in it, a bound one keeps its own meaning, and the results hand typing back.
    var straight = queryPane()
    Focus.handleKey(key(Qt.Key_S, "s"), straight, noRail())
    check("an unbound letter in the listing starts a search of this folder with it",
          straight.searchMode + "|" + straight.searchQuery + "|" + straight.searchDeep, "typing|s|false")
    var bound = queryPane()
    bound.openCursorMenu = function () { return true }
    Focus.handleKey(key(Qt.Key_M, "m"), bound, noRail())
    check("and a bound letter is not typed into one", bound.searchMode, "")
    var resumed = queryPane()
    resumed.searchMode = "results"
    resumed.searchQuery = "scr"
    Focus.handleKey(key(Qt.Key_E, "e"), resumed, noRail())
    check("on the results an unbound printable key goes back to the line",
          resumed.searchMode + "|" + resumed.searchQuery, "typing|scre")
    var menuOn = queryPane()
    menuOn.searchMode = "results"
    menuOn.searchQuery = "scr"
    menuOn.openCursorMenu = function () { menuOn.menued = true; return true }
    Focus.handleKey(key(Qt.Key_M, "m"), menuOn, noRail())
    check("while m still raises the menu on a result rather than being typed",
          menuOn.searchMode + "|" + menuOn.searchQuery + "|" + menuOn.menued, "results|scr|true")
    var deleting = queryPane()
    deleting.searchMode = "results"
    deleting.searchQuery = "scr"
    var did = []
    deleting.act = function (action) { did.push(action) }
    Focus.handleKey(key(Qt.Key_Delete, "\u007f"), deleting, noRail())
    check("and Delete, whose text is DEL, still trashes the result",
          deleting.searchMode + "|" + deleting.searchQuery + "|" + did.join(","), "results|scr|trash")
    var onLine = queryPane()
    onLine.searchMode = "typing"
    onLine.searchQuery = "scr"
    Focus.handleKey(key(Qt.Key_Delete, "\u007f"), onLine, noRail())
    check("and on the line itself DEL is never typed into the query", onLine.searchQuery, "scr")
    var shortened = queryPane()
    shortened.searchMode = "results"
    shortened.searchQuery = "scr"
    Focus.handleKey(key(Qt.Key_Backspace, ""), shortened, noRail())
    check("and backspace shortens the query rather than leaving the results",
          shortened.searchMode + "|" + shortened.searchQuery, "typing|sc")

    // Copy and paste on a query line, the same day's ruling: the clipboard is typed in as one line
    // and the query copied out, while off the line the chords stay the file copy and paste.
    function clip(code, mods) { return { key: code, text: "", modifiers: mods } }
    function clipPane() {
        var p = queryPane()
        p.copied = []
        p.paste = "scr\nipts\n"
        p.opener = { pasteText: function () { return p.paste }, copyText: function (t) { p.copied.push(t) } }
        return p
    }
    var pasting = clipPane()
    pasting.searchMode = "typing"
    pasting.searchQuery = "a "
    Focus.handleKey(clip(Qt.Key_V, Qt.ControlModifier), pasting, noRail())
    check("ctrl+v on the search's line types the clipboard in as one line",
          pasting.searchMode + "|" + pasting.searchQuery, "typing|a scr ipts")
    var copying = clipPane()
    copying.searchMode = "typing"
    copying.searchQuery = "scr"
    Focus.handleKey(clip(Qt.Key_C, Qt.ControlModifier), copying, noRail())
    check("ctrl+c on it copies the query and leaves the line up",
          copying.copied.join(",") + "|" + copying.searchMode, "scr|typing")
    Focus.handleKey(clip(Qt.Key_Insert, Qt.ControlModifier), copying, noRail())
    check("and ctrl+insert copies it the same way", copying.copied.join(","), "scr,scr")
    var spaced = clipPane()
    spaced.searchMode = "typing"
    spaced.searchQuery = "q"
    spaced.paste = " report\r\n"
    Focus.handleKey(clip(Qt.Key_V, Qt.ControlModifier), spaced, noRail())
    check("a pasted leading space is kept and only the trailing line break goes", spaced.searchQuery, "q report")
    var narrowPaste = clipPane()
    narrowPaste.filterTyping = true
    narrowPaste.filterQuery = ""
    Focus.handleKey(clip(Qt.Key_Insert, Qt.ShiftModifier), narrowPaste, noRail())
    check("shift+insert pastes into the filter's line the same way", narrowPaste.filterQuery, "scr ipts")
    var offLine = clipPane()
    offLine.searchMode = "results"
    offLine.searchQuery = "scr"
    var clipped = []
    offLine.act = function (action) { clipped.push(action) }
    Focus.handleKey(clip(Qt.Key_C, Qt.ControlModifier), offLine, noRail())
    check("and on the results ctrl+c is still the file copy", clipped.join(",") + "|" + offLine.copied.length, "copy|0")

    // Tab on the search's query line flips the depth, and it was driven straight into
    // Search.typeKey. Nothing said Focus.handleKey routes Qt.Key_Tab there rather than resolving it
    // through Keymap to focusNext, which is what the very same key means one state away.
    var scoping = queryPane()
    scoping.searchMode = "typing"
    scoping.searchQuery = "scr"
    Focus.handleKey(key(Qt.Key_Tab, "\t"), scoping, noRail())
    check("tab on the search's query line reaches the depth and not the focus switch",
          scoping.searchDeep + "|" + scoping.focusView, "true|list")
    var switching = queryPane()
    Focus.handleKey(key(Qt.Key_Tab, "\t"), switching, noRail())
    check("and with no query line up the same key is the focus switch the sheet draws",
          switching.searchDeep + "|" + switching.focusView, "false|rail")

    // The operator's ruling of 2026-09-23: Ctrl+F searches the open folder alone, Ctrl+Shift+F it
    // and its subfolders, and neither ever walks home or root. Driven through the real key route.
    function chord(shifted) {
        return { key: Qt.Key_F, text: "", modifiers: Qt.ControlModifier | (shifted ? Qt.ShiftModifier : 0) }
    }
    var here = queryPane()
    here.home = "/d"
    here.path = "/d/sub"
    Focus.handleKey(chord(false), here, noRail())
    here.searchQuery = "de"
    Focus.handleKey(key(Qt.Key_Return, ""), here, noRail())
    check("ctrl+f walks the open folder alone, never home", here.walked.join(","), "/d/sub?de")
    var below = queryPane()
    below.home = "/d"
    below.path = "/d/sub"
    Focus.handleKey(chord(true), below, noRail())
    below.searchQuery = "de"
    Focus.handleKey(key(Qt.Key_Return, ""), below, noRail())
    check("ctrl+shift+f walks the open folder and its subfolders", below.walked.join(","), "/d/sub?de+deep")
}
