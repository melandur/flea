.import "../../ui/js/PreviewKeys.js" as PreviewKeys

function run(check) {
    var activated = []
    var viewer = { pdfControlIndex: -1, pdfControls: [], turnPage: function() {},
        zoomBy: function() {}, toggleExpand: function() {}, scrollPage: function() {} }
    for (var i = 0; i < 6; i++) {
        (function(index) {
            viewer.pdfControls.push({enabled: index !== 0 && index !== 2, visible: true,
                activated: function() { activated.push(index) }})
        })(i)
    }
    PreviewKeys.pdfAction("focusNext", viewer)
    check("PDF focus enters first enabled control", viewer.pdfControlIndex, 1)
    PreviewKeys.pdfAction("preview", viewer)
    PreviewKeys.pdfAction("focusPrevious", viewer)
    check("PDF reverse focus wraps to Close", viewer.pdfControlIndex, 5)
    PreviewKeys.pdfAction("open", viewer)
    check("Space and Enter activate the focused PDF controls", activated.join(","), "1,5")
    viewer.pdfControls[5].enabled = false
    PreviewKeys.pdfAction("preview", viewer)
    check("a control disabled after focus never activates", activated.join(","), "1,5")
    viewer.pdfControls[5].visible = false
    PreviewKeys.pdfAction("focusNext", viewer)
    check("forward wrap skips disabled Previous", viewer.pdfControlIndex, 1)
    PreviewKeys.pdfAction("trash", viewer)
    check("listing actions do nothing in PDF context", activated.join(","), "1,5")

    // The operator's ruling of 2026-09-18, "we want that all preview files behave the same way":
    // one contract, two states, and every kind answers both the same. The pane carries only what
    // PreviewKeys.act reads, plus the counters that say which primitive a key reached; Filter's own
    // cursor move needs shown, shownTotal, cursorIndex and showRow, so the fake carries a listing
    // of three rows and records where the cursor went.
    function previewPane(kind, expanded) {
        var pane = { closed: 0, played: 0, expands: 0, scrolled: 0, pages: 0, seeked: 0,
                     followed: 0, cursorIndex: 1, shown: null, shownTotal: 3,
                     path: "/f", kindNames: [], join: function (dir, name) { return dir + "/" + name },
                     rowFor: function () { return { n: "b.txt", i: "text", s: 4, k: 0, d: false } },
                     showRow: function () {} }
        pane.preview = {
            isMedia: kind === "audio" || kind === "video",
            isPdf: kind === "pdf",
            expanded: expanded === true,
            pdfItem: null,
            revealStrip: function () {},
            close: function () { pane.closed += 1 },
            togglePlay: function () { pane.played += 1 },
            toggleExpand: function () { pane.expands += 1; pane.preview.expanded = !pane.preview.expanded },
            scrollPage: function (delta) { pane.scrolled += delta },
            turnPage: function (delta) { pane.pages += delta },
            seek: function (ms) { pane.seeked += ms },
            follow: function () { pane.followed += 1 }
        }
        return pane
    }

    var kinds = ["text", "image", "pdf", "archive", "audio", "video"]
    for (var kind of kinds) {
        // Inset: the vertical pair is the listing's cursor, Left leaves, and Right and Space are
        // the one way in to the filled window.
        var inset = previewPane(kind)
        PreviewKeys.act("cursorDown", inset)
        check("down moves the cursor under a " + kind + " preview", inset.cursorIndex, 2)
        check("and the preview follows it", inset.followed, 1)
        check("and scrolls nothing", inset.scrolled, 0)
        PreviewKeys.act("previewBack", inset)
        check("left closes an inset " + kind + " preview", inset.closed, 1)
        check("and expands nothing", inset.expands, 0)

        var opening = previewPane(kind)
        PreviewKeys.act("previewForward", opening)
        check("right fills the window on a " + kind + " preview", opening.expands, 1)
        check("and closes nothing", opening.closed, 0)

        var spaced = previewPane(kind)
        PreviewKeys.act("preview", spaced)
        check("space fills the window on a " + kind + " preview", spaced.expands + (spaced.preview.expanded ? 10 : 0), 11)
        PreviewKeys.act("preview", spaced)
        check("and space again brings the inset one back", spaced.expands + (spaced.preview.expanded ? 10 : 0), 2)
        check("neither press closes a " + kind + " preview", spaced.closed, 0)
        PreviewKeys.act("previewBack", spaced)
        check("and the inset one it left closes on left", spaced.closed, 1)

        // Expanded: the arrows move the surface, and none of the four leaves it.
        var big = previewPane(kind, true)
        PreviewKeys.act("cursorDown", big)
        PreviewKeys.act("cursorDown", big)
        PreviewKeys.act("cursorUp", big)
        check("the expanded " + kind + " surface scrolls on up and down", big.scrolled, 1)
        check("and the listing cursor stays where it was", big.cursorIndex, 1)
        check("and the preview reloads nothing", big.followed, 0)
        PreviewKeys.act("previewForward", big)
        PreviewKeys.act("previewBack", big)
        check("the expanded " + kind + " surface is never closed by an arrow", big.closed, 0)
        check("nor collapsed by one", big.expands, 0)
        // Escape leaves from either state, which is the one key that never changes meaning.
        PreviewKeys.act("escape", big)
        check("escape closes an expanded " + kind + " preview", big.closed, 1)
    }

    // The horizontal pair in the expanded state, where the kinds differ because their content does:
    // a PDF has pages, a media file a playhead, and the rest have neither.
    var reader = previewPane("pdf", true)
    PreviewKeys.act("previewForward", reader)
    PreviewKeys.act("previewForward", reader)
    PreviewKeys.act("previewBack", reader)
    check("right pages an expanded PDF forward and left back", reader.pages, 1)
    check("and neither seeks", reader.seeked, 0)

    var tune = previewPane("audio", true)
    PreviewKeys.act("previewForward", tune)
    PreviewKeys.act("previewBack", tune)
    PreviewKeys.act("previewBack", tune)
    check("right and left seek an expanded media preview", tune.seeked, -PreviewKeys.SEEK_MS)
    check("and turn no page", tune.pages, 0)

    // An image, a text file and an archive listing have no page and no playhead, so the pair asks
    // for a page turn their surface declines rather than borrowing the other axis's meaning.
    var still = previewPane("image", true)
    PreviewKeys.act("previewForward", still)
    check("the pair asks an expanded image for nothing it cannot do", still.seeked + still.closed + still.expands, 0)

    // Space no longer plays, so p does, and only where there is something to play.
    var playing = previewPane("audio")
    PreviewKeys.act("playPause", playing)
    check("p plays and pauses a media preview", playing.played, 1)
    check("and closes nothing", playing.closed, 0)
    check("and expands nothing", playing.expands, 0)
    var mute = previewPane("image")
    PreviewKeys.act("playPause", mute)
    check("p does nothing to a still preview", mute.played + mute.closed, 0)
}
