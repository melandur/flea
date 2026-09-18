import QtQuick
import qs.Commons
import "." as Flea
import "js/Facts.js" as Facts

// The canvas's Archive tile at Quick Look size: the name, the count the index gave, then the
// entries. Its own file because ui/Preview.qml reached its 400-line hard cap when the expanded
// surface arrived, and of the five kinds the overlay draws this was the only one still written
// inline; every other one is already a component or a Loader source.
Column {
    id: root

    property string path: ""
    // The backend's meta answer for the open archive, null until it lands.
    property var meta: null

    spacing: Theme.spacing.gap

    // The expanded surface's Up and Down, one member a press, and where they have left it.
    function scrollBy(steps) { list.scrollBy(steps) }
    readonly property int offset: list.offset

    // corner: a filename is arbitrary text, so PlainText, the same rule every name on this surface follows.
    Text {
        width: parent.width
        text: root.path.substring(root.path.lastIndexOf("/") + 1)
        color: Theme.color.foreground
        font.family: Theme.font.family
        font.pixelSize: Theme.font.body
        textFormat: Text.PlainText
        elide: Text.ElideMiddle
    }

    Text {
        width: parent.width
        text: Facts.archiveLine(root.meta)
        color: Theme.color.muted
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        textFormat: Text.PlainText
    }

    Flea.PreviewArchive {
        id: list
        width: parent.width
        height: parent.height - y
        meta: root.meta
    }
}
