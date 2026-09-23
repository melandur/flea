import QtQuick
import Quickshell
import Quickshell.Io
import "." as Flea
import "js/Delimited.js" as Delimited

// A delimited body drawn as a grid: the first record pinned as the header, one delegate per record on
// screen, and the record count as the model, the same integer-count rule the listing follows. Cells
// are parsed as their row scrolls in, so opening a large file costs one walk for the record offsets.
Item {
    id: root

    // The body as a var and not a string, so a delegate reading it is handed the engine's own value
    // and never a fresh copy of a file that can be hundreds of megabytes.
    property var body: ""
    property string name: ""
    // A spreadsheet's path instead of a body: flea --sheet answers its first sheet as CSV, and that
    // answer is drawn exactly as a .csv would be.
    property string sheet: ""
    property bool failed: false
    property bool streamDone: false
    property bool exitDone: false
    property var converted: ""
    readonly property bool loading: root.sheet.length > 0 && !(root.streamDone && root.exitDone)
    readonly property var source: root.sheet.length > 0 ? (root.failed ? "" : root.converted) : root.body
    // The column frame's form: caption type, no gutter, no scrolling, only the first records read.
    property bool compact: false
    // The walk costs about 420 ms a million quoted records, measured on 2026-09-23, so it stops here.
    property int maxRecords: 1000000
    // More columns than this are not drawn; a sheet that wide is not read in a preview.
    readonly property int maxColumns: 128
    readonly property int maxCellChars: root.compact ? 24 : 40

    readonly property string delim: root.sheet.length > 0 ? "," : Delimited.sniff(root.source, root.name)
    readonly property var starts: Delimited.recordStarts(root.source, root.delim, root.maxRecords)
    readonly property int records: Math.max(0, root.starts.length - 1)
    // The walk stopped at maxRecords rather than at the end of the body.
    readonly property bool truncated: root.records > 0 && root.starts[root.records] < root.source.length
    readonly property var chars: Delimited.columnChars(root.source, root.starts, root.delim, root.maxCellChars)
    readonly property int columns: Math.min(root.maxColumns, root.chars.length)
    readonly property var numeric: Delimited.numericColumns(root.source, root.starts, root.delim)
    readonly property int fontSize: root.compact ? Theme.font.caption : Theme.font.body
    readonly property real cellPad: Theme.spacing.gap * 2
    readonly property var widths: root.chars.slice(0, root.columns).map(function (c) {
        return Math.ceil(Math.max(3, c) * glyph.advanceWidth) + root.cellPad
    })
    readonly property real gutter: root.compact ? 0 : Math.ceil(String(root.records).length * glyph.advanceWidth) + root.cellPad
    readonly property real rowWidth: root.widths.reduce(function (a, b) { return a + b }, root.gutter)
    readonly property real rowHeight: Math.ceil(glyph.height) + Theme.spacing.gap

    // A new body opens at its first record: the header is an overlay, so a view left at 0 hides row 1.
    onStartsChanged: Qt.callLater(list.positionViewAtBeginning)

    // For ui/PreviewText.qml: the scrolled surface, which its Up and Down and ui/Ipc.qml both drive.
    readonly property alias view: list

    // The row count asked for is one past the walk's own limit, so a sheet cut short still says so.
    function convert() {
        if (converter.running) { converter.running = false; return }
        root.converted = ""
        root.failed = false
        root.streamDone = false
        root.exitDone = false
        if (root.sheet.length === 0) return
        converter.forSheet = root.sheet
        converter.command = [Quickshell.env("FLEA_BIN") || "flea", "--sheet", root.sheet, String(root.maxRecords + 1)]
        converter.running = true
    }
    onSheetChanged: root.convert()
    Component.onCompleted: root.convert()

    // A run for a sheet no longer shown is killed, and its exit starts the one that is.
    Process {
        id: converter
        property string forSheet: ""
        stdout: StdioCollector {
            onStreamFinished: if (converter.forSheet === root.sheet) { root.converted = this.text; root.streamDone = true }
        }
        onExited: function (code) {
            if (converter.forSheet !== root.sheet) { Qt.callLater(root.convert); return }
            root.failed = code !== 0
            root.exitDone = true
        }
    }

    TextMetrics {
        id: glyph
        font.family: Theme.font.family
        font.pixelSize: root.fontSize
        text: "0"
    }

    ListView {
        id: list
        anchors.fill: parent
        clip: true
        interactive: !root.compact
        boundsBehavior: Flickable.StopAtBounds
        // A vertical ListView leaves its width alone, so a sheet wider than the frame pans sideways.
        contentWidth: Math.max(width, root.rowWidth)
        flickableDirection: Flickable.AutoFlickDirection
        headerPositioning: ListView.OverlayHeader
        reuseItems: true
        model: Math.max(0, root.records - 1)

        Flea.FastScrollHandler {
            parent: list
            flickable: list
            enabled: !root.compact
        }

        header: Rectangle {
            width: list.contentWidth
            height: root.records > 0 ? root.rowHeight : 0
            z: 2
            color: Theme.color.surface
            visible: root.records > 0

            RecordRow { record: 0; header: true }

            Rectangle {
                anchors.bottom: parent.bottom
                width: parent.width
                height: Theme.spacing.hairline
                color: Theme.color.muted
            }
        }

        delegate: RecordRow {
            required property int index
            record: index + 1
        }

        footer: Text {
            visible: root.truncated
            height: visible ? root.rowHeight : 0
            leftPadding: root.gutter + Theme.spacing.gap
            verticalAlignment: Text.AlignVCenter
            text: "First " + root.records.toLocaleString(Qt.locale(), "f", 0) + " rows"
            color: Theme.color.muted
            font.family: Theme.font.family
            font.pixelSize: root.fontSize
            textFormat: Text.PlainText
        }
    }

    Text {
        anchors.centerIn: parent
        visible: root.sheet.length > 0 && !root.loading && !root.failed && root.records === 0
        text: "This sheet is empty."
        color: Theme.color.muted
        font.family: Theme.font.family
        font.pixelSize: root.fontSize
        textFormat: Text.PlainText
    }

    component RecordRow: Rectangle {
        id: rec
        property int record: 0
        property bool header: false
        readonly property var cells: rec.record < root.records
            ? Delimited.fields(root.source, root.starts[rec.record], root.starts[rec.record + 1], root.delim) : []
        width: list.contentWidth
        height: root.rowHeight
        // Every other record shaded, so a row read across a wide sheet stays on its own line.
        color: !rec.header && rec.record % 2 === 0 ? Qt.rgba(Theme.color.foreground.r, Theme.color.foreground.g,
                                                             Theme.color.foreground.b, 0.05) : "transparent"

        Row {
            anchors.verticalCenter: parent.verticalCenter

            Text {
                visible: !root.compact
                width: root.gutter
                rightPadding: Theme.spacing.gap
                horizontalAlignment: Text.AlignRight
                text: rec.header ? "" : String(rec.record)
                color: Theme.color.muted
                opacity: 0.6
                font.family: Theme.font.family
                font.pixelSize: root.fontSize
                textFormat: Text.PlainText
            }

            Repeater {
                model: root.columns

                // corner: file contents are arbitrary text, so PlainText, the same rule every name follows.
                Text {
                    required property int index
                    width: root.widths[index]
                    leftPadding: Theme.spacing.gap
                    rightPadding: Theme.spacing.gap
                    horizontalAlignment: root.numeric[index] === true ? Text.AlignRight : Text.AlignLeft
                    text: Delimited.shown(rec.cells[index] || "")
                    color: rec.header ? Theme.color.accent : Theme.color.foreground
                    font.family: Theme.font.family
                    font.pixelSize: root.fontSize
                    font.bold: rec.header
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                    maximumLineCount: 1
                }
            }
        }
    }
}
