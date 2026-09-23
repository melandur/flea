import QtQuick
import Quickshell.Io
import "." as Flea
import "js/Kinds.js" as Kinds
import "js/TextChunks.js" as TextChunks

// Text previews: FileView reads the whole file, so a row over the gate is refused, not truncated.
Item {
    id: root

    property bool active: false
    property string path: ""
    property real size: 0

    // FileView reads the whole file into memory, so this is the largest read a preview will start:
    // 500 MiB, the operator's ruling of 2026-09-23. Only the runs on screen are laid out, so the cost
    // left is the read and the cut, measured at 37 ms for 16 MB, 219 ms and 0.8 GB resident for
    // 100 MB, and 1 s, a 0.5 s frame and 3 GB resident for 500 MB. It was 1 MiB while one Text laid
    // out the whole body, which cost 1.6 s for 4 MiB.
    readonly property int maxBytes: 524288000
    // Right on a refusal lifts the gate up to this ceiling. At the gate itself it refuses nothing
    // Right could load, so the prompt stays dormant until the two are set apart again.
    readonly property int forceMaxBytes: 524288000
    property bool forced: false
    readonly property bool forcible: root.size <= root.forceMaxBytes
    readonly property bool tooLarge: root.size > (root.forced ? root.forceMaxBytes : root.maxBytes)
    onPathChanged: root.forced = false
    onActiveChanged: if (!root.active) root.forced = false
    function loadAnyway() { if (root.tooLarge && root.forcible) root.forced = true }
    property bool readFailed: false

    // The whole body, emptied as the preview closes so the window it leaves holds no file, and the
    // runs of it the list below draws, only the ones on screen ever laid out.
    // A CSV and its cousins draw as a table instead of as runs, which the rest of this file never
    // needs to know: the scrolled surface is whichever of the two is on screen. A spreadsheet is a
    // table too, and is never read here at all: the table asks flea --sheet for its first sheet.
    readonly property bool tabular: Kinds.isTabular(root.path)
    readonly property bool sheet: Kinds.isSheet(root.path)
    readonly property string text: root.active && !root.sheet ? file.text() : ""
    readonly property var runs: root.tabular ? [] : TextChunks.split(root.text)
    readonly property Flickable flick: root.tabular ? table.view : textFlick
    // For ui/Ipc.qml: the drawn body's box and its text.
    readonly property Item bodyItem: root.flick
    function shownText() { return root.sheet ? String(table.source) : root.text }
    // The expanded overlay's Up and Down, one row of the listing's own height a press, which is the
    // step ui/PdfViewer.qml scrolls a page by; a body shorter than the frame cannot move at all. A
    // list of runs estimates the heights it has not laid out, so its top is originY and not 0.
    function scrollBy(steps) {
        var f = root.flick
        var bottom = f.originY + Math.max(0, f.contentHeight - f.height)
        f.contentY = Math.max(f.originY, Math.min(bottom, f.contentY + steps * Theme.rowHeight))
    }
    // For ui/Ipc.qml and the expanded surface both: where the body has been scrolled to.
    readonly property real scrollY: root.flick.contentY - root.flick.originY
    readonly property string status: {
        if (root.tooLarge) return "This file is too large to preview."
        if (root.readFailed || table.failed) return "This file could not be read."
        if (root.sheet) return table.loading ? "loading" : "ready"
        return file.loaded ? "ready" : "loading"
    }

    visible: root.active

    FileView {
        id: file
        path: (root.active && !root.tooLarge && !root.sheet) ? root.path : ""
        printErrors: false
        onLoadFailed: root.readFailed = true
        onPathChanged: root.readFailed = false
    }

    ListView {
        id: textFlick
        anchors.fill: parent
        clip: true
        visible: !root.tabular && !root.tooLarge && !root.readFailed
        boundsBehavior: Flickable.StopAtBounds
        model: root.runs
        reuseItems: true

        FastScrollHandler {
            parent: textFlick
            flickable: textFlick
        }

        delegate: Text {
            required property string modelData
            width: textFlick.width
            text: modelData
            // MarkdownText resolves inline image references, so a downloaded README would fetch from
            // the network on cursor movement; the canvas asks for the file verbatim in any case.
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            color: Theme.color.foreground
            font.family: Theme.font.family
            font.pixelSize: Theme.font.body
        }
    }

    Flea.PreviewTable {
        id: table
        anchors.fill: parent
        visible: root.tabular && !root.tooLarge && !root.readFailed && !table.failed
        body: root.tabular ? root.text : ""
        sheet: root.active && root.sheet && !root.tooLarge ? root.path : ""
        name: root.path
    }

    Column {
        anchors.centerIn: parent
        visible: root.tooLarge || root.readFailed || table.failed
        spacing: Theme.spacing.gap

        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: root.status
            color: Theme.color.muted
            font.family: Theme.font.family
            font.pixelSize: Theme.font.body
            textFormat: Text.PlainText
        }

        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            visible: root.tooLarge && root.forcible
            text: "Press right to load it anyway."
            color: Theme.color.muted
            font.family: Theme.font.family
            font.pixelSize: Theme.font.caption
            textFormat: Text.PlainText
        }
    }
}
