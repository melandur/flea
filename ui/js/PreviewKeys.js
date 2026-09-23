.pragma library

.import "Filter.js" as Filter

// What the preview overlay does with a key, split out of Focus.js at its 300-line hard cap the
// same way ui/js/Trash.js was: Focus.js decides which surface owns a key, and this is the surface.

// Left/Right's seek step, Task 22's operator ruling; act is the only reader.
var SEEK_MS = 5000

function pdfAction(action, viewer) {
    var controls = viewer.pdfControls
    if (action === "focusNext" || action === "focusPrevious") {
        var step = action === "focusPrevious" ? -1 : 1
        var next = viewer.pdfControlIndex
        if (next < 0) next = step > 0 ? -1 : 0
        for (var i = 0; i < controls.length; i++) {
            next = (next + step + controls.length) % controls.length
            if (controls[next].enabled && controls[next].visible) {
                viewer.pdfControlIndex = next
                break
            }
        }
    } else if (action === "open" || action === "preview") {
        var control = controls[viewer.pdfControlIndex]
        if (control && control.enabled && control.visible) control.activated()
    // The inline column's own horizontal pair reaches here under both spellings: previewBack and
    // previewForward are what the map binds now, and parent and pageForward are the browsing pair
    // the column answers with a page turn too.
    } else if (action === "previewBack" || action === "parent") viewer.turnPage(-1)
    else if (action === "previewForward" || action === "pageForward") viewer.turnPage(1)
    else if (action === "zoomOut") viewer.zoomBy(-1)
    else if (action === "zoomIn") viewer.zoomBy(1)
    else if (action === "expand") viewer.toggleExpand()
    else if (action === "cursorDown" || action === "cursorUp") viewer.scrollPage(action === "cursorDown" ? 1 : -1)
}

// A directory has no preview kind of its own, so Space on one is a silent no-op rather than an error.
function open(root) {
    var row = root.rowFor(root.cursorIndex)
    if (row && !row.d)
        root.preview.open(root.join(root.path, row.n), row.i, row.s, root.kindNames[row.k] || "")
}

// The preview's whole keyboard, one contract for every kind it can draw: the operator's ruling of
// 2026-09-18, "we want that all preview files behave the same way". Two states, and the state is
// the only thing that changes what an arrow means.
//
//   inset      Up/Down move the listing cursor and the preview follows it, Left leaves the preview,
//              Right and Space make it fill the window.
//   expanded   Up/Down scroll the surface, Left and Right move the content itself: a PDF's page and
//              a media file's playhead. Space does nothing: Escape goes straight back to the
//              folder, skipping the inset surface, the operator's ruling of 2026-09-23.
//
// Escape closes outright from either state, because a preview must never need a particular key to
// leave it. What this reverses is two rulings, and both are written down rather than re-litigated:
// Space closed the preview from 2026-09-11 ("pressing space a second time should close the
// preview, just like Finder does"), and Left seeked in media and turned a PDF's page while it
// closed every plain kind, so the one key meant two things depending on the row under the cursor.
// Left still closes the preview the operator is looking at; it is only the expanded surface that
// takes it for the content, which is the state the operator asked for it in.
//
// A surface with nothing to move in an axis answers nothing rather than borrowing the other
// meaning: a fitted image cannot pan, wrapped text has no horizontal overflow, and a media file has
// no vertical one. ui/Preview.qml's scrollPage and turnPage are the primitives, and each of them
// self-guards on the kind, which is why there is no kind test around either call here.
//
// Any key reveals the media strip, even one that does nothing else, matching "move the mouse or
// press anything" from Task 22's ruling.
function act(action, root) {
    root.preview.revealStrip()
    var expanded = root.preview.expanded === true
    switch (action) {
    // Inset, the cursor is the listing's and the preview follows it; expanded, it is the surface's.
    case "cursorDown":
        if (expanded) { root.preview.scrollPage(1); return }
        Filter.moveCursor(root, 1); follow(root); return
    case "cursorUp":
        if (expanded) { root.preview.scrollPage(-1); return }
        Filter.moveCursor(root, -1); follow(root); return
    // Space fills the window and nothing else. It used to bring the inset surface back too, which
    // the operator ruled out on 2026-09-23 ("remove the space key to go back"): from the filled
    // window Escape is the way out, straight to the folder.
    case "preview": if (!expanded) root.preview.toggleExpand(); return
    case "previewBack":
        if (!expanded) { root.preview.close(); return }
        if (root.preview.isMedia) root.preview.seek(-SEEK_MS)
        else root.preview.turnPage(-1)
        return
    // A text file refused for size is the one exception to both states: Right loads it anyway, and
    // the next Right is the ordinary one.
    case "previewForward":
        if (root.preview.textRefused) { root.preview.loadAnyway(); return }
        if (!expanded) { root.preview.toggleExpand(); return }
        if (root.preview.isMedia) root.preview.seek(SEEK_MS)
        else root.preview.turnPage(1)
        return
    case "escape": root.preview.close(); return
    // Space no longer plays, so playback has its own key; it self-guards, because p reaches this
    // only in the media context and a still image has nothing to play.
    case "playPause":
        if (root.preview.isMedia) root.preview.togglePlay()
        return
    // MediaMute rule 5: the flag is the preview's to flip, and it silences without pausing.
    case "mute": root.preview.toggleMute(); return
    // The PDF's chrome strip is the one surface with controls of its own, so Tab walks them and
    // Enter presses the one it is on; ui/PdfViewer.qml routes its own keys through here, which is
    // what makes a PDF answer the four arrows the way every other kind does.
    case "focusNext":
    case "focusPrevious":
    case "open":
        if (root.preview.isPdf && root.preview.pdfItem) pdfAction(action, root.preview.pdfItem)
        return
    }
}

// The row under the moved cursor, handed to Preview.follow so a held key settles before it reloads.
function follow(root) {
    var row = root.rowFor(root.cursorIndex)
    if (row && !row.d)
        root.preview.follow(root.join(root.path, row.n), row.i, row.s, root.kindNames[row.k] || "")
}
