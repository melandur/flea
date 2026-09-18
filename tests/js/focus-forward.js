.import "../../ui/js/Focus.js" as Focus
.import "../../ui/js/PreviewKeys.js" as PreviewKeys

function closed() {
    return { active: false, isMedia: false, isPdf: false }
}

function pdfOpen() {
    return { active: true, isMedia: false, isPdf: true }
}

function mediaOpen() {
    return { active: true, isMedia: true, isPdf: false }
}

function rightKey() {
    return { key: Qt.Key_Right, text: "", modifiers: Qt.NoModifier }
}

// rowFor records its only input so browse-forward cannot grow beyond Pane's held viewport.
function pane(row, view) {
    var p = {
        cursorIndex: 37,
        focusView: view ? view : "list",
        viewMode: "list",
        searchMode: "",
        preview: closed(),
        shareBrowser: { active: false },
        rowsRead: [],
        rowFor: function (index) { p.rowsRead.push(index); return row }
    }
    return p
}

function handlePane(row) {
    var p = pane(row)
    p.filterTyping = false
    p.inputAt = 0
    p.rowsAt = 0
    p.trashArmedAt = 0
    p.shown = null
    p.said = ""
    p.renameEditor = function () { return null }
    p.message = function (text) { p.said = text }
    return p
}

function sidebar() {
    return { renameEditor: function () { return null } }
}

function pdfPreview() {
    return {
        active: true,
        isMedia: false,
        isPdf: true,
        expanded: true,
        page: 0,
        revealStrip: function () {},
        turnPage: function (delta) { this.page += delta }
    }
}

function run(check) {
    var right = rightKey()

    var directory = pane({ d: true, t: false, i: "folder" })
    check("right enters the directory under the cursor", Focus.lookup(right, directory), "open")
    check("directory lookup reads only the held cursor row", directory.rowsRead.join(","), "37")

    var extensionlessSymlinkDirectory = pane({ d: false, p: 0o120777, i: "folder", t: false })
    check("right enters an extensionless symlink to a directory", Focus.lookup(right, extensionlessSymlinkDirectory), "open")
    check("extensionless symlink lookup reads only the held cursor row", extensionlessSymlinkDirectory.rowsRead.join(","), "37")

    var thumbnailableSymlinkDirectory = pane({ d: false, p: 0o120777, i: "folder", t: true })
    check("right enters a thumbnailable symlink to a directory", Focus.lookup(right, thumbnailableSymlinkDirectory), "open")
    check("thumbnailable symlink lookup reads only the held cursor row", thumbnailableSymlinkDirectory.rowsRead.join(","), "37")

    var file = pane({ d: false, p: 0o100644, i: "folder", t: false })
    check("right previews an ordinary file even with a folder icon", Focus.lookup(right, file), "preview")
    check("file lookup reads only the held cursor row", file.rowsRead.join(","), "37")

    var empty = pane(null)
    check("right is silent without a held cursor row", Focus.lookup(right, empty), "")
    check("empty lookup still asks only for the held cursor row", empty.rowsRead.join(","), "37")

    var unloaded = handlePane(null)
    check("right on an unloaded row is consumed without a filter hint",
          Focus.handleKey(right, unloaded, sidebar()) + "|" + unloaded.said, "true|")
    check("unloaded handling still asks only for the held cursor row", unloaded.rowsRead.join(","), "37")

    // The rail's Right is an open that takes the keyboard with it, which is the 2026-09-18 ruling:
    // opening a place from the sidebar used to leave the cursor there and cost a second press to
    // reach the listing it had just opened. Enter keeps the old behaviour, so a walk down PLACES can
    // still open one place after another.
    var rail = pane(null, "rail")
    check("right opens the selected rail row and goes in", Focus.lookup(right, rail), "openInto")
    check("rail lookup does not inspect the hidden listing", rail.rowsRead.length, 0)

    var railHandled = handlePane(null)
    railHandled.focusView = "rail"
    var railSidebar = sidebar()
    railSidebar.entries = [{ kind: "favourite" }]
    railSidebar.cursorIndex = 0
    railSidebar.opened = 0
    railSidebar.activate = function () { railSidebar.opened += 1 }
    check("and the press both opens the place and hands the listing the keyboard",
          Focus.handleKey(right, railHandled, railSidebar) + "|" + railSidebar.opened + "|" + railHandled.focusView,
          "true|1|list")

    // A rail with nothing in it has nothing to open, so the keyboard stays where it is rather than
    // being handed to a listing the press never reached.
    var emptyRail = handlePane(null)
    emptyRail.focusView = "rail"
    var emptySidebar = sidebar()
    emptySidebar.entries = []
    emptySidebar.cursorIndex = 0
    emptySidebar.activate = function () { emptySidebar.opened = true }
    check("an empty rail keeps the keyboard",
          Focus.handleKey(right, emptyRail, emptySidebar) + "|" + emptyRail.focusView, "true|rail")

    var share = pane({ d: false })
    share.shareBrowser.active = true
    check("right activates the selected share", Focus.lookup(right, share), "open")
    check("share lookup does not inspect the hidden listing", share.rowsRead.length, 0)

    var pdf = pane(null)
    pdf.preview = pdfOpen()
    check("right is the preview's own forward over an open PDF", Focus.lookup(right, pdf), "previewForward")
    check("PDF lookup does not inspect the hidden listing", pdf.rowsRead.length, 0)

    // Expanded, that forward is the page itself; inset it fills the window, which previewkeys.js
    // covers for all six kinds. The reader here starts expanded because the page is what is checked.
    var reader = pdfPreview()
    PreviewKeys.act("previewForward", { preview: reader })
    PreviewKeys.act("previewForward", { preview: reader })
    PreviewKeys.act("previewBack", { preview: reader })
    check("right advances the expanded PDF and left retreats it", reader.page, 1)

    var media = pane(null)
    media.preview = mediaOpen()
    check("right is the same forward over media rather than browsing in", Focus.lookup(right, media), "previewForward")
    check("media lookup does not inspect the hidden listing", media.rowsRead.length, 0)
}
