import QtQuick
import qs.Commons
import "." as Flea
import "js/Facts.js" as Facts

// The Archive tile's frame: the entries the wire carried, as many as the frame has room for, and the
// count it could not name. One row per line box, less the "+ N more" line, which always has to be
// visible: a list that ran past the frame would hide the very number it exists to state.
Item {
    id: root

    // The meta answer for the archive row, or null before one has arrived.
    property var meta: null

    readonly property real lineHeight: Math.round(Theme.font.caption * Theme.lineBoxRatio)
    readonly property int shown: Math.max(0, Math.floor(height / lineHeight) - 1)
    // Which member the frame starts at, the expanded overlay's Up and Down: one row a press through
    // the names the wire carried, which is src/backend/archivelist.rs ARCHIVE_NAME_CAP of them and
    // never the whole index, so the "+ N more" line below goes on stating what no scroll can reach.
    property int offset: 0
    readonly property int names: root.meta && root.meta.names ? root.meta.names.length : 0

    function scrollBy(steps) {
        root.offset = Math.max(0, Math.min(Math.max(0, root.names - root.shown), root.offset + steps))
    }

    // A new archive is a new index, so the frame starts at its first member however the last was left.
    onMetaChanged: root.offset = 0

    Column {
        anchors.fill: parent
        clip: true
        spacing: 0

        Repeater {
            model: Facts.archiveEntries(root.meta, root.offset + root.shown).slice(root.offset)

            delegate: Row {
                required property var modelData
                width: root.width
                height: root.lineHeight
                spacing: Theme.spacing.gap

                Flea.Glyph {
                    anchors.verticalCenter: parent.verticalCenter
                    width: Theme.font.caption
                    height: Theme.font.caption
                    name: modelData.d ? "folder" : "file"
                    color: Theme.color.muted
                }

                // corner: an archive holds arbitrary names, so PlainText, the same rule every name follows.
                Text {
                    text: modelData.n
                    color: Theme.color.muted
                    font.family: Theme.font.family
                    font.pixelSize: Theme.font.caption
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                }
            }
        }

        Text {
            height: root.lineHeight
            visible: Facts.archiveMore(root.meta, root.offset + root.shown) > 0
            text: "+ " + Facts.archiveMore(root.meta, root.offset + root.shown) + " more"
            color: Theme.color.muted
            opacity: 0.6
            font.family: Theme.font.family
            font.pixelSize: Theme.font.caption
            textFormat: Text.PlainText
        }
    }
}
