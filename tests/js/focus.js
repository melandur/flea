.import "../../ui/js/Focus.js" as Focus
.import "../../ui/js/Keymap.js" as Keymap
.import "filterfixture.js" as Fixture

// Focus.lookup is where a key is discarded for being meaningless in the current state, and a wrong
// gate there is silent: the key simply does nothing, and no suite but this one would notice.

function pane(preview, viewMode) {
    return {
        focusView: "list", shown: null,
        viewMode: viewMode ? viewMode : "list",
        chooseView: function (mode) { this.viewMode = mode },
        searchMode: "",
        preview: preview
    }
}

// A pane showing search results, which is the one state the two sort keys are taken away in.
function searching(preview) {
    var p = pane(preview)
    p.searchMode = "results"
    return p
}

// The rail with focus, the one view m means anything in, carrying the sink for what it answers.
function railPane() {
    var p = pane(closed())
    p.focusView = "rail"
    p.said = ""
    p.message = function (text, isError) { p.said = text }
    return p
}

// A pane and a rail for the eject key: the rail's rows and cursor, and a sink for what releaseChosen
// is handed, since that call is the whole of what the key must produce.
function ejectPane(view, path, entries, cursor) {
    var p = listPane(true)
    p.focusView = view
    p.path = path
    p.sidebar = { entries: entries, cursorIndex: cursor, released: [],
                  releaseChosen: function (action, key) { this.released.push(action + ":" + key) } }
    return p
}

// The listing with focus and only what Focus.act's menu case reads: the pane's own opener, which
// answers whether a row was under the cursor, and the sink for what the key says when none was.
function listPane(hasRow) {
    var p = pane(closed())
    p.opened = 0
    p.said = ""
    p.openCursorMenu = function () { p.opened += 1; return hasRow }
    p.message = function (text, isError) { p.said = text }
    return p
}

function closed() {
    return { active: false, isMedia: false, isPdf: false }
}

// Only the members Focus.handleKey touches on its way to the terminal chord, and the counter for
// the one call it must make. The button lives in the chrome above both views, so the chord has to
// answer from the rail as well as the list rather than being swallowed by whichever view has focus.
function chromePane(view) {
    return {
        focusView: view, viewMode: "list", searchMode: "", filterTyping: false,
        inputAt: 0, rowsAt: 0, trashArmedAt: 0, asked: 0, copied: 0, said: "", shown: null,
        preview: closed(),
        message: function (text, isError) { this.said = text },
        shareBrowser: { active: false },
        sidebar: { renameEditor: function () { return null } },
        renameEditor: function () { return null },
        // What ui/Pane.qml's own act() does with an action, so a route that reaches the pane's
        // dispatch instead of the interception is visible here rather than throwing.
        act: function (action) { Focus.act(action, this) },
        openTerminal: function () { this.asked += 1 },
        copyDirPath: function () { this.copied += 1 }
    }
}

function pdfOpen() {
    return { active: true, isMedia: false, isPdf: true }
}

function mediaOpen() {
    return { active: true, isMedia: true, isPdf: false }
}

// An image, a text file or an archive listing: open, and neither of the two kinds that seek.
function plainOpen() {
    return { active: true, isMedia: false, isPdf: false }
}

function key(code, text, modifiers) {
    return { key: code, text: text, modifiers: modifiers }
}

// Only the members the escape case reads. Search.cancel and Pane.escapePressed both record rather
// than act, because what is being checked is the order they are reached in.
function escaper(query, retreated) {
    var p = pane(closed())
    p.filterQuery = query
    p.filterTyping = false
    p.retreated = retreated
    p.cancelled = 0
    p.searchRunning = false
    p.backend = { searchcancel: function () { p.cancelled += 1 } }
    p.escapePressed = function () { p.retreated += 1 }
    return p
}

function run(check) {
    var none = Qt.NoModifier
    var shift = Qt.ShiftModifier

    var minus = key(Qt.Key_Minus, "-", none)
    var plus = key(Qt.Key_Plus, "+", shift)
    var e = key(Qt.Key_E, "e", none)
    var left = key(Qt.Key_Left, "", none)
    var right = key(Qt.Key_Right, "", none)

    // The three left the table with every other bare letter, so the PDF's zoom and expand are the
    // strip's own controls now. Focus.lookup keeps its gate, which is what would scope them again.
    check("e no longer expands an open PDF", Focus.lookup(e, pane(pdfOpen())), "")
    check("minus no longer zooms an open PDF", Focus.lookup(minus, pane(pdfOpen())), "")
    check("plus no longer zooms an open PDF", Focus.lookup(plus, pane(pdfOpen())), "")

    // The three are silent everywhere else too, as they always were.
    check("e is discarded while browsing", Focus.lookup(e, pane(closed())), "")
    check("minus is discarded while browsing", Focus.lookup(minus, pane(closed())), "")
    check("plus is discarded while browsing", Focus.lookup(plus, pane(closed())), "")
    check("e is discarded over a media preview", Focus.lookup(e, pane(mediaOpen())), "")
    check("minus is discarded over a media preview", Focus.lookup(minus, pane(mediaOpen())), "")

    // Left and Right serve the preview and the grid's own sideways step, and while browsing they are
    // the navigation itself: up a level, and into the row under the cursor.
    // The 2026-09-18 ruling, "all preview files behave the same way": the pair resolves to one
    // action per direction in every preview context, and what it moves is ui/js/PreviewKeys.js act's
    // reading of the expanded state, not a second binding per kind. Escape keeps closing beside it.
    check("left is the preview's own back on a plain kind", Focus.lookup(left, pane(plainOpen())), "previewBack")
    check("escape closes a plain preview too", Focus.lookup(key(Qt.Key_Escape, "", none), pane(plainOpen())), "escape")
    check("right is the preview's own forward on a plain kind", Focus.lookup(right, pane(plainOpen())), "previewForward")
    check("a PDF answers the same pair", Focus.lookup(left, pane(pdfOpen())) + "/" + Focus.lookup(right, pane(pdfOpen())), "previewBack/previewForward")
    check("and so does media", Focus.lookup(left, pane(mediaOpen())) + "/" + Focus.lookup(right, pane(mediaOpen())), "previewBack/previewForward")
    check("left goes up a level in the list", Focus.lookup(left, pane(closed())), "parent")
    check("left still steps a grid tile", Focus.lookup(left, pane(closed(), "grid")), "cursorLeft")
    check("right still steps a grid tile", Focus.lookup(right, pane(closed(), "grid")), "cursorRight")
    // The Default preset suppresses the letter pair, so it reaches neither the grid's sideways
    // step nor the tree, and the arrows above are the whole of its navigation.
    var hKey = key(Qt.Key_H, "h", none)
    var lKey = key(Qt.Key_L, "l", none)
    check("h does not step a grid tile on the default preset", Focus.lookup(hKey, pane(closed(), "grid")), "")
    check("l does not step a grid tile on the default preset", Focus.lookup(lKey, pane(closed(), "grid")), "")
    check("and h does not climb the tree on the default preset", Focus.lookup(hKey, pane(closed())), "")


    // The path bar, the tab keys and the filter went with the bare letters and punctuation; the
    // chrome and the menus still reach all three, and Focus.lookup keeps the gates that scoped them.
    var colon = key(Qt.Key_Colon, ":", shift)
    check("colon no longer reaches the path bar", Focus.lookup(colon, pane(closed())), "")
    var newTab = key(Qt.Key_T, "t", none)
    check("t no longer opens a tab", Focus.lookup(newTab, pane(closed())), "")

    var slash = key(Qt.Key_Slash, "/", none)
    check("slash no longer opens the filter in the list view", Focus.lookup(slash, pane(closed())), "")
    check("slash no longer opens the filter in the grid view", Focus.lookup(slash, pane(closed(), "grid")), "")
    check("slash is discarded in the columns view", Focus.lookup(slash, pane(closed(), "columns")), "")
    // A walk replaces the listing a filter would be narrowing, and its strip covers the header, so
    // / goes quiet there exactly as s and S do.
    check("slash is discarded while a search owns the header",
          Focus.lookup(slash, searching(closed())), "")

    // And starting one drops a filter that was standing, or the results are narrowed by a query that
    // was written against the directory listing they just replaced.
    var walker = escaper("scr", 0)
    walker.started = 0
    walker.searchMode = ""
    Focus.act("search", walker)
    check("f clears a standing filter before the query line opens",
          walker.filterQuery + "|" + walker.searchMode, "|typing")

    // Esc unwinds one thing at a time, least destructive first. A filter costs nothing to clear, a
    // selection is work, so the filter goes first and a second esc is what drops the selection.
    var unwind = escaper("scr", 0)
    Focus.act("escape", unwind)
    check("esc clears a standing filter before it touches the selection",
          unwind.filterQuery + "|" + unwind.retreated, "|0")
    Focus.act("escape", unwind)
    check("and a second esc reaches the selection", unwind.retreated, 1)
    var walking = escaper("", 0)
    walking.searchMode = "results"
    walking.searchRunning = true
    Focus.act("escape", walking)
    check("a running search still outranks both", walking.cancelled, 1)
    var typing = escaper("", 0)
    typing.filterTyping = true
    Focus.act("escape", typing)
    check("esc while the query line has the caret closes it", typing.filterTyping, false)

    var activeStatus = escaper("needle", 0)
    var statusEscapes = 0
    activeStatus.statusBar = {escapePressed: function () { statusEscapes += 1; return true }}
    activeStatus.searchMode = "results"
    activeStatus.searchRunning = true
    Focus.act("escape", activeStatus)
    check("filter consumes Escape before search or transfer", activeStatus.filterQuery + "|" + activeStatus.cancelled + "|" + statusEscapes, "|0|0")
    Focus.act("escape", activeStatus)
    check("focused search consumes Escape before transfer", activeStatus.cancelled + "|" + statusEscapes, "1|0")
    activeStatus.searchMode = ""
    Focus.act("escape", activeStatus)
    check("status consumes Escape before marks", statusEscapes + "|" + activeStatus.retreated, "1|0")
    activeStatus.statusBar.escapePressed = function () { return false }
    Focus.act("escape", activeStatus)
    check("idle status lets Escape clear marks", activeStatus.retreated, 1)

    // The search strip covers the header whole, so its mark cannot be seen moving, and a sort ends
    // the walk in the backend. Both keys go silent while a search is up rather than cancelling one
    // from a key the sheet never advertised there.
    var sortNext = key(Qt.Key_S, "s", none)
    var sortReverse = key(Qt.Key_S, "S", shift)
    check("s no longer sorts while browsing", Focus.lookup(sortNext, pane(closed())), "")
    check("S no longer reverses while browsing", Focus.lookup(sortReverse, pane(closed())), "")
    check("s is discarded over a search result", Focus.lookup(sortNext, searching(closed())), "")
    check("S is discarded over a search result", Focus.lookup(sortReverse, searching(closed())), "")

    // m is the one key into the menu a right click raises, in both views: the rail's rows and the
    // listing's row menu, which had no key at all, so it is routed by which view has focus.
    var m = key(Qt.Key_M, "m", none)
    var home = { label: "Home", group: "favorite", kind: "favorite", path: "/home/user" }
    check("m raises the menu while the rail has focus", Focus.lookup(m, railPane()), "menu")
    check("m raises the menu in the list too, so the row menu has a key", Focus.lookup(m, pane(closed())), "menu")

    // The network dialog kept no key of its own when the table was cut back to the arrows and the
    // Ctrl chords, so the rail's "+" mark and the menu are what reach it; act still routes it from
    // either view, which is what keys.toml promised from the first commit while Focus.js made it
    // rail-only (GM, 2026-09-11).
    var ctrl = Qt.ControlModifier
    check("the network dialog has no key left", Focus.lookup(key(Qt.Key_A, "a", none), pane(closed())), "")
    for (var preset of Keymap.PRESETS) {
        Keymap.setPreset(preset)
        check(preset + " Grid Left moves between tiles", Focus.lookup(left, pane(closed(), "grid")), "cursorLeft")
        check(preset + " Grid Right moves between tiles", Focus.lookup(right, pane(closed(), "grid")), "cursorRight")
        check(preset + " Grid PDF Right keeps the preview's own forward", Focus.lookup(right, pane(pdfOpen(), "grid")), "previewForward")
    }
    Keymap.setPreset("default")
    var dialled = listPane(true)
    dialled.sidebar = { asked: 0, addRequested: function () { this.asked += 1 } }
    Focus.act("addNetwork", dialled)
    check("and act opens it through the rail's own signal", dialled.sidebar.asked, 1)

    // Finder's Cmd+1/2/3: the same property the chrome's three buttons write, so they follow.
    var viewed = listPane(true)
    Focus.act("viewGrid", viewed)
    var grid = viewed.viewMode
    Focus.act("viewColumns", viewed)
    var cols = viewed.viewMode
    Focus.act("viewList", viewed)
    check("ctrl 3, 2 and 1 pick the grid, the columns and the list", grid + "|" + cols + "|" + viewed.viewMode, "grid|columns|list")

    // The seam itself. A case that only messaged was indistinguishable from a wired one on this
    // side of the suite, which is how the whole feature stayed unreachable through a green run, so
    // the check names the request that has to reach the backend and asserts no message replaces it.
    var folder = listPane(true)
    folder.path = "/d"
    folder.made = []
    folder.backend = { mkdir: function (path) { folder.made.push(path) } }
    Focus.act("newFolder", folder)
    check("ctrl shift n asks the backend for a folder in the listed directory, and says nothing",
          folder.made.join(",") + "|" + folder.said, "/d|")

    // Finder's Cmd+E in a listing: the removable volume the listing is inside, whose verdict is
    // Mounts.railMenu's, released through the same releaseChosen a chosen menu row takes. The rail's
    // own half of the key is ui/js/RailKeys.js's, and tests/js/railkeys.js drives it.
    var stick = { label: "128GB", group: "device", kind: "volume", device: "/dev/sda1", path: "/run/media/user/128GB", mounted: true, removable: true }
    var inside = ejectPane("list", "/run/media/user/128GB/photos", [home, stick], 0)
    Focus.act("eject", inside)
    check("ctrl e in a listing inside the volume ejects that volume, whatever the rail cursor is on",
          inside.sidebar.released.join(",") + "|" + inside.said, "eject:/dev/sda1|")
    var outside = ejectPane("list", "/home/user/Documents", [home, stick], 1)
    Focus.act("eject", outside)
    check("ctrl e in a listing on the internal disk says so, even with the rail cursor on the stick",
          outside.sidebar.released.length + "|" + outside.said,
          "0|This is not inside a removable volume.")

    // The listing's m goes through the pane, which says whether a delegate was under the cursor; an
    // empty directory and a filter that hides every row both get the sentence rather than silence.
    var listing = listPane(true)
    Focus.act("menu", listing)
    check("m opens the menu under the cursor row, and says nothing over it",
          listing.opened + "|" + listing.said, "1|")
    var bare = listPane(false)
    Focus.act("menu", bare)
    check("m with no row under the cursor says why instead of swallowing the key",
          bare.opened + "|" + bare.said, "1|No row under the cursor to open a menu on.")

    // PR 34's chord. The context-menu row and Ctrl+T raise the same terminal, and the rail owns its
    // own keys, so the one route both views share is the interception in handleKey above the views.
    var terminalKey = key(Qt.Key_T, "\u0014", ctrl)
    check("ctrl t no longer carries the terminal but the split instead",
          Focus.lookup(terminalKey, pane(closed())), "toggleDual")
    // The menu row's own route: ui/ContextMenu.qml fires the action into ui/Pane.qml's act(), which
    // never sees handleKey's interception, and this is the dispatch that was missing when it did not.
    var fromMenu = chromePane("list")
    Focus.act("openTerminal", fromMenu)
    check("the menu row reaches the same terminal through act", fromMenu.asked + "|" + fromMenu.said, "1|")

    var menu = listPane(true)
    var menuRequests = []
    menu.path = "/d"
    menu.cursorIndex = 0
    menu.rowFor = function () { return {n: "selected.txt"} }
    menu.selectedIndices = function () { return [] }
    menu.join = function (parent, name) { return parent + "/" + name }
    menu.sticky = function () {}
    menu.backend = {
        duplicate: function (path, id) { menuRequests.push("duplicate:" + id) },
        trash: function (rows, id) { menuRequests.push("trash:" + id) },
        extract: function (path, dest, id) { menuRequests.push("extract:" + id) },
        compress: function (paths, dest, format, id) { menuRequests.push(paths.join(",") + ":" + id) }
    }
    menu.moveToDropbox = function (id) { menuRequests.push("dropbox:" + id) }
    menu.openConvert = function (id) { menuRequests.push("convert:" + id) }
    var selectedPaths = ["/d/captured.txt", "/d/second.txt"]
    Focus.act("copy", menu, 42, selectedPaths)
    check("menu dispatch copies the captured paths without another asynchronous lookup",
          menu.clipboard.paths.join(",") + "|" + menu.clipboard.moving, "/d/captured.txt,/d/second.txt|false")
    Focus.act("cut", menu, 42, selectedPaths)
    check("menu cut keeps the captured paths and move intent", menu.clipboard.moving, true)
    for (var action of ["duplicate", "trash", "extract", "dropbox", "convert", "compress:zip"])
        Focus.act(action, menu, 42, selectedPaths)
    check("menu dispatch keeps the identity through each mutation consumer",
          menuRequests.join("|"),
          "duplicate:42|trash:42|extract:42|dropbox:42|convert:42|/d/captured.txt,/d/second.txt:42")

    // Y copies root.path, the same thing Ctrl+T opens a terminal on, so it answers from the rail
    // too; without the interception RailKeys.act ate it and the key did nothing and said nothing.
    var copyKey = key(Qt.Key_Y, "Y", shift)
    check("Y no longer carries the folder path", Focus.lookup(copyKey, pane(closed())), "")
    // The interception itself is unchanged and is what the menu row still reaches, from either view.
    var copyList = chromePane("list")
    Focus.act("copydirpath", copyList)
    check("the menu row still copies the folder path through act", copyList.copied, 1)
    var copyRail = chromePane("rail")
    Focus.act("copydirpath", copyRail)
    check("and does so with the rail focused as well", copyRail.copied, 1)

    var shareOwner = chromePane("list")
    var otherPane = chromePane("list")
    var sharedBrowser = {active: true, owner: shareOwner}
    shareOwner.shareBrowser = sharedBrowser
    otherPane.shareBrowser = sharedBrowser
    check("a share listing belongs to the pane that requested it", Focus.shareBrowserHere(shareOwner), true)
    check("another pane keeps its own key context while the shared listing is open", Focus.shareBrowserHere(otherPane), false)
    sharedBrowser.active = false
    check("closing the share listing releases its owner's keys", Focus.shareBrowserHere(shareOwner), false)
}
