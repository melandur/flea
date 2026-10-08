.import "../../ui/js/Search.js" as Search
.import "../../ui/js/Nav.js" as Nav
.import "../../ui/js/PreviewKeys.js" as PreviewKeys

function run(check) {
    // Rule 6: a count that is still growing says so, beside the count.
    check("a running walk with matches pairs the count with the state", Search.note(9, true, false), "9 found · still scanning")
    check("and a finished one is the count alone", Search.note(9, false, false), "9 found")
    check("a running walk with nothing yet says it is working", Search.note(0, true, false), "searching")
    check("a finished walk with nothing says done", Search.note(0, false, false), "done")
    check("a cancelled walk with nothing says stopped", Search.note(0, false, true), "stopped")

    // V7: the SearchFilter board's own status cell, the count beside the state that changes it.
    check("a running walk pairs what it found with what it scanned",
          Search.statusLine(true, 12, 4120, 300), "12 found · Searching, 4,120 scanned")
    check("a finished walk reports what it found and how long it took",
          Search.statusLine(false, 1, 18204, 412), "1 found in 0.4 s")
    check("and nothing found still says so", Search.statusLine(false, 0, 18204, 412), "0 found in 0.4 s")


    check("the home prefix reads as a tilde", Search.scope("/home/gm/Work/claude/flea", "/home/gm"), "~/Work/claude/flea")
    check("home itself is the bare tilde", Search.scope("/home/gm", "/home/gm"), "~")
    check("a path outside home keeps its own form", Search.scope("/usr/share", "/home/gm"), "/usr/share")
    check("a deep search says it reaches below the folder",
          Search.scope("/home/gm/Work", "/home/gm", true), "~/Work and subfolders")

    // The strip's own right edge, which the board draws as the pair esc really is from here.
    check("the strip says esc stops the walk and then leaves", Search.wayOut(true), "esc cancels, then returns")
    check("and once the walk is done esc only leaves", Search.wayOut(false), "esc returns")


    // The pointer's own contract: a double click on a result reveals it, and opens a row anywhere
    // else. ui/js/Tap.js asks this rather than deciding it, so the rule lives beside the reveal.
    check("a double click on a search result takes the operator to the file",
          Search.activateAction({ searchMode: "results" }), "reveal")
    check("a double click on an ordinary row still opens it",
          Search.activateAction({ searchMode: "" }), "open")
    check("a double click while the query line is still up opens the row under it",
          Search.activateAction({ searchMode: "typing" }), "open")

    // A walk with no matches yet is still working, so the area keeps the crawl rather than flashing empty.
    check("matches make the listing ready", Search.listingState({ searchRunning: true }, 3), "ready")
    check("a running walk with nothing yet is still loading", Search.listingState({ searchRunning: true }, 0), "loading")
    check("a stopped walk with nothing is empty", Search.listingState({ searchRunning: false }, 0), "empty")

    // The query line's own keys, beside the transitions they drive. Only the members run, close,
    // typed and backspace touch are stubbed, and open counts so a re-list can be told from none.
    function typing(query) {
        var sent = []
        return {
            searchMode: "typing", searchQuery: query, searchRunning: false, searchScanned: 0,
            searchCancelled: false, path: "/d", showHidden: false, total: 0, held: 0, rows: [],
            kindNames: [], cursorIndex: 0, listingState: "ready", opened: 0, sent: sent,
            home: "", searchFrom: "", relisted: "", searchDeep: false,
            clearSelection: function () {},
            open: function (path) { this.opened += 1 },
            openWithoutHistory: function (path) { this.relisted = path },
            backend: { search: function (path, query, hidden, shallow) {
                sent.push(path + "?" + query + (shallow ? "" : "+deep"))
            } }
        }
    }
    function press(code, text) { return { key: code, text: text, modifiers: Qt.NoModifier } }
    var line = typing("scr")
    Search.typeKey(press(Qt.Key_E, "e"), line)
    check("a printable key extends the query", line.searchQuery, "scre")
    Search.typeKey(press(Qt.Key_Backspace, ""), line)
    check("backspace shortens it", line.searchQuery, "scr")
    check("a key that means nothing on the line is still consumed by it",
          Search.typeKey(press(Qt.Key_Left, ""), line) + "|" + line.searchQuery, "true|scr")
    Search.typeKey(press(Qt.Key_Return, ""), line)
    check("enter commits the walk", line.searchMode + "|" + line.sent.join(","), "results|/d?scr")
    var abandoned = typing("scr")
    Search.typeKey(press(Qt.Key_Escape, ""), abandoned)
    check("escape abandons the line without re-listing, since no walk ran",
          abandoned.searchMode + "|" + abandoned.searchQuery + "|" + abandoned.opened, "||0")
    // The operator's ruling of 2026-09-23: the walk is the open folder, never home and never root.
    var underHome = typing("scr")
    underHome.home = "/home/u"
    underHome.path = "/home/u/Downloads"
    Search.typeKey(press(Qt.Key_Return, ""), underHome)
    check("the walk is sent the pane's own folder alone, never home",
          underHome.sent.join(",") + "|" + underHome.path, "/home/u/Downloads?scr|/home/u/Downloads")
    check("where the search was started from is remembered", underHome.searchFrom, "/home/u/Downloads")
    Search.close(underHome)
    check("leaving the results returns there, and never as a history entry",
          underHome.relisted + "|" + underHome.opened, "/home/u/Downloads|0")

    var again = typing("scr")
    again.path = "/home/u/Downloads"
    Search.typeKey(press(Qt.Key_Return, ""), again)
    Search.start(again, true)
    again.searchQuery = "other"
    Search.typeKey(press(Qt.Key_Return, ""), again)
    check("a second search from the results walks the same folder",
          again.sent.join(","), "/home/u/Downloads?scr,/home/u/Downloads?other+deep")
    Search.close(again)
    check("so esc returns where the operator began", again.relisted, "/home/u/Downloads")

    // Ctrl+F is this folder, Ctrl+Shift+F this folder and every one below it.
    var shallow = typing("")
    Search.start(shallow, false)
    check("the plain chord opens a search of the folder alone", shallow.searchDeep, false)
    var deep = typing("")
    Search.start(deep, true)
    check("the shifted chord opens a search of its subfolders too", deep.searchDeep, true)

    // Tab on the query line flips the depth the chord chose, and the strip's "in <scope>" reads it.
    var here = typing("scr")
    here.path = "/home/u/Downloads"
    Search.typeKey(press(Qt.Key_Tab, "\t"), here)
    check("tab on the query line reaches into the subfolders", here.searchDeep, true)
    // Both presses land while the line still has the caret, which is the only state ui/js/Focus.js
    // handleKey routes to typeKey at all: after enter the mode is results and the key is the rail's.
    Search.typeKey(press(Qt.Key_Backtab, "\t"), here)
    check("and the key flips back to the folder alone", here.searchDeep, false)
    Search.typeKey(press(Qt.Key_Tab, "\t"), here)
    Search.typeKey(press(Qt.Key_Return, ""), here)
    check("and the walk descends from the same folder",
          here.sent.join(",") + "|" + here.path, "/home/u/Downloads?scr+deep|/home/u/Downloads")

    var blank = typing("")
    Search.typeKey(press(Qt.Key_Return, ""), blank)
    check("enter on an empty line closes it rather than walking for nothing",
          blank.searchMode + "|" + blank.sent.length, "|0")

    // Results follow the typing: each edit restarts ui/Pane.qml's debounce, and its firing walks the
    // query as it stands. Enter then only hands the keyboard over, since that walk is already out.
    function debounced(query) {
        var p = typing(query)
        p.restarts = 0
        p.searchLive = { restart: function () { p.restarts += 1 }, stop: function () {} }
        p.searchWalked = ""
        p.searchPending = 0
        p.listInFlight = false
        return p
    }
    var live = debounced("sc")
    Search.typeKey(press(Qt.Key_R, "r"), live)
    check("a keystroke restarts the debounce rather than walking", live.restarts + "|" + live.sent.length, "1|0")
    Search.live(live)
    check("the debounce walks the query as typed and the caret stays up",
          live.sent.join(",") + "|" + live.searchMode, "/d?scr|typing")
    check("and the walk's results count as a search listing while typing", Search.activateAction(live), "reveal")
    Search.live(live)
    check("a debounce over an unchanged query walks nothing more", live.sent.length, 1)
    Search.typeKey(press(Qt.Key_Return, ""), live)
    check("enter after a live walk hands over the keyboard without walking again",
          live.sent.join(",") + "|" + live.searchMode, "/d?scr|results")

    var flipped = debounced("scr")
    Search.live(flipped)
    Search.typeKey(press(Qt.Key_Tab, "\t"), flipped)
    Search.live(flipped)
    check("tab re-walks the same query one level deeper", flipped.sent.join(","), "/d?scr,/d?scr+deep")

    var emptied = debounced("s")
    emptied.path = "/home/u/Downloads"
    Search.live(emptied)
    Search.typeKey(press(Qt.Key_Backspace, ""), emptied)
    Search.live(emptied)
    check("emptying the line puts the folder back and keeps the caret",
          emptied.relisted + "|" + emptied.searchMode + "|" + Search.listed(emptied), "/home/u/Downloads|typing|false")

    var waiting = debounced("scr")
    waiting.listInFlight = true
    Search.live(waiting)
    check("a re-list still out defers the walk rather than racing it",
          waiting.sent.length + "|" + waiting.restarts, "0|1")

    var twice = debounced("a")
    Search.live(twice)
    twice.searchQuery = "ab"
    Search.live(twice)
    check("each walk is counted until its listed line comes back", twice.searchPending, 2)
    Search.close(twice)
    check("and leaving keeps the count, since those lines are still on the wire", twice.searchPending, 2)

    // The terminal searched line. The backend ranks the rows in the statement before it writes that
    // line, so a client still drawing the walk's discovery order resolves every destructive key
    // against a listing it is not showing: trash on the highlighted row took another file.
    // Only the members Search.ranked touches, and the backend stub records the wire, because the
    // window request is the whole of it. The cursor starts off row 0 so a reset can be told from none.
    function results() {
        var p = {
            windowSize: 200,
            thumbState: "stale",
            dirSizeState: "stale",
            cursor: 7,
            cleared: 0,
            sent: []
        }
        p.clearSelection = function () { p.cleared += 1 }
        p.setCursor = function (index) { p.cursor = index }
        p.backend = { window: function (start, count) { p.sent.push("window " + start + " " + count) } }
        return p
    }

    var reordered = results()
    Search.ranked(reordered)
    check("a ranked walk drops the thumbnail cache, whose keys are row indices",
          reordered.thumbState === "stale", false)
    check("and the directory-size cache, keyed the same way",
          reordered.dirSizeState === "stale", false)
    check("and clears a selection of indices that now name other files", reordered.cleared, 1)
    check("and puts the cursor on the highest-ranked row rather than a stale index", reordered.cursor, 0)
    check("and re-reads the window, which is what keeps trash on the row that is drawn",
          reordered.sent.join(","), "window 0 200")

    // Going anywhere from the results ends the search, the operator's ruling of 2026-10-08: enter on
    // a result folder, back, forward and up each land in a plain folder, never a stale query strip.
    function searching() {
        var p = {
            searchMode: "results", searchQuery: "cases", searchFrom: "/d", searchWalked: "here:cases",
            searchRunning: false, searchScanned: 0, path: "/d", listInFlight: false, history: ["/a"],
            forwardHistory: [], pendingSelect: "", cursorIndex: 1, listed: "", opened: [],
            searchLive: { stop: function () {}, restart: function () {} },
            rows: [{ n: "sub/batch_01", d: true }, { n: "sub/batch_01/cases.txt", d: false, i: "text-plain", s: 9, k: 0 }],
            kindNames: ["Text"],
            rowFor: function (i) { return this.rows[i] },
            join: function (base, name) { return base + "/" + name },
            message: function () {},
            openWithoutHistory: function (path) { this.listed = path },
            preview: { opened: "", open: function (path) { this.opened = path } }
        }
        p.open = function (path) { Nav.open(p, path) }
        return p
    }
    var entered = searching()
    Nav.open(entered, "/d/sub/batch_01")
    check("opening a result folder ends the search and lists the folder",
          entered.searchMode + "|" + entered.searchQuery + "|" + entered.listed, "||/d/sub/batch_01")
    check("and back returns to the folder the search walked", entered.history.join(","), "/a,/d")
    var backed = searching()
    Nav.back(backed)
    check("back from the results ends the search too", backed.searchMode + "|" + backed.listed, "|/a")
    var climbed = searching()
    Nav.parent(climbed)
    check("up from the results backs out to the walked folder, not its parent",
          climbed.searchMode + "|" + climbed.listed, "|/d")

    // A preview opened over the results lands the pane beside the file it showed once it closes.
    var peeking = searching()
    PreviewKeys.open(peeking)
    check("space on a result previews the file by its full path", peeking.preview.opened, "/d/sub/batch_01/cases.txt")
    Search.unpeek(searching())
    check("another pane's close moves nothing", peeking.searchMode, "results")
    PreviewKeys.open(peeking)
    Search.unpeek(peeking)
    check("closing it ends the search in the file's own folder, the file selected",
          peeking.searchMode + "|" + peeking.listed + "|" + peeking.pendingSelect,
          "|/d/sub/batch_01|/d/sub/batch_01/cases.txt")
    Search.unpeek(peeking)
    check("and the peek is spent, so a later close moves nothing", peeking.history.join(","), "/a,/d")
    var plain = searching()
    plain.searchMode = ""
    PreviewKeys.open(plain)
    Search.unpeek(plain)
    check("a preview over an ordinary folder closes where it was", plain.listed, "")
}
