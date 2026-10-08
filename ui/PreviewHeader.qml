import QtQuick
import qs.Commons
import "." as Flea
import "js/Format.js" as Format
import "js/Icons.js" as Icons

// The Quick Look's title strip: the file's mark, its name, and the folder it is in, the operator's
// ruling of 2026-10-08. The PDF viewer's own top bar is the model, so the two strips read as one:
// the same chrome height, mark, caption type and hairline under it. A PDF keeps its own bar, which
// carries the page controls this one has no use for.
Rectangle {
    id: root

    property string path: ""
    property string iconName: ""
    // The listing's home, so the folder reads ~/... the way the window chrome draws it.
    property string home: ""

    readonly property string name: root.path.substring(root.path.lastIndexOf("/") + 1)
    readonly property string folder: {
        var cut = root.path.lastIndexOf("/")
        return cut <= 0 ? "/" : Format.tilde(root.path.substring(0, cut), root.home)
    }

    height: Theme.chromeHeight
    color: "transparent"

    Rectangle {
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        height: Theme.spacing.hairline
        color: Theme.color.foreground
        opacity: 0.12
    }

    Flea.Glyph {
        id: mark
        anchors.left: parent.left
        anchors.leftMargin: Theme.spacing.rowPaddingX
        anchors.verticalCenter: parent.verticalCenter
        width: Theme.chromeMarkSize
        height: Theme.chromeMarkSize
        name: Icons.glyphFor(root.iconName)
        color: Theme.color.foreground
    }

    // corner: a filename is arbitrary text, so PlainText, the same rule every name on this surface
    // follows. The name keeps its room and the folder gives way, eliding from its middle, so the
    // two ends of a deep path both survive a narrow window.
    Text {
        id: nameText
        anchors.left: mark.right
        anchors.leftMargin: Theme.spacing.gap
        anchors.verticalCenter: parent.verticalCenter
        width: Math.min(implicitWidth, root.width - x - Theme.spacing.rowPaddingX)
        text: root.name
        color: Theme.color.foreground
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        textFormat: Text.PlainText
        elide: Text.ElideRight
    }

    Text {
        anchors.left: nameText.right
        anchors.leftMargin: 2 * Theme.spacing.gap
        anchors.right: parent.right
        anchors.rightMargin: Theme.spacing.rowPaddingX
        anchors.verticalCenter: parent.verticalCenter
        text: root.folder
        color: Theme.color.muted
        font.family: Theme.font.family
        font.pixelSize: Theme.font.caption
        textFormat: Text.PlainText
        elide: Text.ElideMiddle
    }
}
