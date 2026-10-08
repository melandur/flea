import QtQuick
import qs.Commons
import "." as Flea

// The Quick Look declining, or an archive whose index could not be read: a mark over the sentence,
// never a bare surface. Split out of ui/Preview.qml at its 400-line hard cap, with no behaviour.
Item {
    id: root

    property bool failed: false
    property string status: ""

    Column {
        anchors.centerIn: parent
        width: parent.width - 2 * Theme.spacing.rowPaddingX
        spacing: Theme.spacing.gap

        Flea.Glyph {
            anchors.horizontalCenter: parent.horizontalCenter
            // The overlay declining is a pane state standing alone, which States.dc.html draws at 40.
            maxSize: Theme.stateMarkSize
            width: Theme.stateMarkSize
            height: Theme.stateMarkSize
            name: root.failed ? "alert" : "file"
            color: root.failed ? Theme.color.error : Theme.color.muted
        }

        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            text: root.status
            color: root.failed ? Theme.color.foreground : Theme.color.muted
            font.family: Theme.font.family
            font.pixelSize: Theme.font.body
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
        }
    }
}
