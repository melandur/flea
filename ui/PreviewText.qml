import QtQuick
import Quickshell.Io
import "js/TextChunks.js" as TextChunks

// Text previews: FileView reads the whole file, so a row over the gate is refused, not truncated.
Item {
    id: root

    property bool active: false
    property string path: ""
    property int size: 0

    // FileView reads the whole file into memory, so this is the largest read a preview will start.
    readonly property int maxBytes: 1048576
    // Right on the refusal lifts the gate up to this ceiling: laying out plain text blocks the UI
    // thread, measured at 1.6 s for 4 MiB, 3 s for 16 MiB and 12 s for 64 MiB, so past it no key
    // loads the file at all.
    readonly property int forceMaxBytes: 16777216
    property bool forced: false
    readonly property bool forcible: root.size <= root.forceMaxBytes
    readonly property bool tooLarge: root.size > (root.forced ? root.forceMaxBytes : root.maxBytes)
    onPathChanged: root.forced = false
    onActiveChanged: if (!root.active) root.forced = false
    function loadAnyway() { if (root.tooLarge && root.forcible) root.forced = true }
    property bool readFailed: false

    // The whole body, emptied as the preview closes so the window it leaves holds no file, and the
    // runs of it the list below draws, only the ones on screen ever laid out.
    readonly property string text: root.active ? file.text() : ""
    readonly property var runs: TextChunks.split(root.text)
    // For ui/Ipc.qml: the drawn body's box and its text.
    readonly property Item bodyItem: textFlick
    function shownText() { return root.text }
    // The expanded overlay's Up and Down, one row of the listing's own height a press, which is the
    // step ui/PdfViewer.qml scrolls a page by; a body shorter than the frame cannot move at all. A
    // list of runs estimates the heights it has not laid out, so its top is originY and not 0.
    function scrollBy(steps) {
        var bottom = textFlick.originY + Math.max(0, textFlick.contentHeight - textFlick.height)
        textFlick.contentY = Math.max(textFlick.originY, Math.min(bottom, textFlick.contentY + steps * Theme.rowHeight))
    }
    // For ui/Ipc.qml and the expanded surface both: where the body has been scrolled to.
    readonly property real scrollY: textFlick.contentY - textFlick.originY
    readonly property string status: {
        if (root.tooLarge) return "This file is too large to preview."
        if (root.readFailed) return "This file could not be read."
        return file.loaded ? "ready" : "loading"
    }

    visible: root.active

    FileView {
        id: file
        path: (root.active && !root.tooLarge) ? root.path : ""
        printErrors: false
        onLoadFailed: root.readFailed = true
        onPathChanged: root.readFailed = false
    }

    ListView {
        id: textFlick
        anchors.fill: parent
        clip: true
        visible: !root.tooLarge && !root.readFailed
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

    Column {
        anchors.centerIn: parent
        visible: root.tooLarge || root.readFailed
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
