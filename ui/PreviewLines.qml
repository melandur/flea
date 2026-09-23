import QtQuick
import Quickshell.Io
import qs.Commons
import "." as Flea
import "js/Kinds.js" as Kinds

// The first lines of a text file, in the preview column's frame. The canvas is explicit that this
// invents no highlighting: "first lines verbatim, mono" for text, "line numbers muted, text
// foreground, still just text" for code. A gutter is the only difference between the two.
Item {
    id: root

    property string path: ""
    property bool active: false
    property real size: 0
    // Code gets the muted number gutter; plain text does not.
    property bool numbered: false

    // The same gate ui/PreviewText.qml uses: FileView reads the whole file, so a row over it is
    // refused rather than truncated.
    readonly property int maxBytes: 1048576
    // The sheet reader stops at the rows the frame shows, so a large sheet costs no more than a small one.
    readonly property bool tooLarge: !root.sheet && root.size > root.maxBytes
    // More than a small frame can show is wasted work, so only this many are ever built into rows.
    readonly property int maxLines: 14

    // A CSV and its cousins draw their first records as a grid rather than as raw lines, and a
    // spreadsheet draws its first sheet's the same way, through flea --sheet and never read here.
    readonly property bool tabular: Kinds.isTabular(root.path)
    readonly property bool sheet: Kinds.isSheet(root.path)
    readonly property var lines: {
        if (!root.active || root.tooLarge || !file.loaded)
            return []
        return file.text().split("\n").slice(0, root.maxLines)
    }

    // Surfaced so the column can show the canvas's Error tile instead of an empty frame.
    property bool readFailed: false
    // True when the reader has settled with nothing to put in the frame, so the column stands the
    // kind's mark in rather than draw a bordered empty box: refused for size, or a file with no bytes.
    readonly property bool blank: root.tooLarge
        || (root.active && !root.readFailed && (root.sheet ? !grid.loading && grid.records === 0
                                                           : file.loaded && file.text().length === 0))
    readonly property bool loading: root.active && !root.tooLarge && !root.readFailed && (root.sheet ? grid.loading : !file.loaded)
    onPathChanged: root.readFailed = false

    FileView {
        id: file
        path: (root.active && !root.tooLarge && !root.sheet) ? root.path : ""
        printErrors: false
        onLoadFailed: root.readFailed = true
        onPathChanged: root.readFailed = false
    }

    // The gutter's width is the widest number it will draw, so the text column never shifts as it scrolls.
    TextMetrics {
        id: gutterMetrics
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        text: "00 "
    }

    Flea.PreviewTable {
        id: grid
        anchors.fill: parent
        anchors.margins: Theme.spacing.gap
        visible: root.tabular
        compact: true
        maxRecords: root.maxLines
        body: root.tabular && root.lines.length > 0 ? file.text() : ""
        sheet: root.active && root.sheet ? root.path : ""
        name: root.path
        onFailedChanged: if (failed) root.readFailed = true
    }

    Column {
        anchors.fill: parent
        anchors.margins: Theme.spacing.gap
        clip: true
        spacing: 0
        visible: !root.tabular

        Repeater {
            model: root.lines

            delegate: Row {
                required property string modelData
                required property int index
                spacing: 0

                Text {
                    visible: root.numbered
                    width: root.numbered ? gutterMetrics.width : 0
                    text: String(index + 1)
                    horizontalAlignment: Text.AlignRight
                    color: Theme.color.muted
                    opacity: 0.6
                    font.family: Theme.font.family
                    font.pixelSize: Theme.font.caption
                    textFormat: Text.PlainText
                }

                // corner: file contents are arbitrary text, so PlainText, the same rule every name follows.
                Text {
                    text: "  " + modelData
                    color: Theme.color.foreground
                    font.family: Theme.font.family
                    font.pixelSize: Theme.font.caption
                    textFormat: Text.PlainText
                    // One line per row, never wrapped: the canvas shows the file's own line breaks.
                    elide: Text.ElideRight
                }
            }
        }
    }
}
