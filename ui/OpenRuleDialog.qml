import QtQuick
import qs.Commons
import "." as Flea
import "js/OpenWith.js" as OpenWith
import "js/SettingsOpen.js" as SettingsOpen

// Settings > File types' own card: one ending, one application. The Convert popup family's card,
// field and button language, and ui/OpenWithDialog.qml's own list of installed applications, which
// is where the catalogue comes from; this card asks for that half alone, because a rule is about an
// ending and there is no file under the cursor here to name handlers for.
FocusScope {
    id: root
    anchors.fill: parent
    visible: root.opened

    property bool opened: false
    // The pane's own backend, and the catalogue it answers with. The walk covers every applications
    // directory on the box, so it is asked for once and then held by ui/SettingsPanel.qml, which
    // draws the application's name on every rule row from the same answer.
    property var backend: null
    property var installed: []
    property bool busy: false
    property string errorText: ""
    property int requestId: 0
    property Item focusHolder: null
    // The ending this card was opened on, "" for a rule being added; kept so the panel can replace
    // the right row when the operator edits the application and leaves the ending alone.
    property string editing: ""
    // The application that rule already names, so editing one opens on its own row in the catalogue
    // rather than on whatever happens to sort first.
    property string editingApp: ""
    property int cursor: 0

    signal saved(string ends, string app)
    signal catalogue(var apps)
    signal closed()

    readonly property var matches: OpenWith.matching(root.installed, search.text)
    readonly property var chosen: root.matches[root.cursor]
    readonly property string ending: SettingsOpen.normalised(field.text)
    readonly property bool endingValid: SettingsOpen.isEnding(root.ending)
    readonly property bool canSubmit: !root.busy && root.endingValid && root.chosen !== undefined
    // For ui/Ipc.qml: what the card holds, which is not legible off a screenshot.
    readonly property var cardItem: card
    readonly property var fieldItem: field
    readonly property var applicationsItem: list

    readonly property int viewportRows: 7

    function open(ends, app, holder) {
        root.focusHolder = holder
        root.editing = String(ends || "")
        root.editingApp = String(app || "")
        root.cursor = 0
        field.text = root.editing
        search.text = ""
        root.opened = true
        list.contentY = 0
        root.cursor = root.cursorFor(root.editingApp)
        field.forceActiveFocus()
        field.selectAll()
        root.ask()
    }

    // Asked when the section is shown and again if the card opens without an answer yet, so the
    // application's own name is on every rule row rather than its id; a catalogue already in hand is
    // the same catalogue, and ui/SettingsPanel.qml drops it when the panel closes.
    function ask() {
        if (!root.backend || root.busy || root.installed.length > 0) return
        root.requestId = Math.max(1, Date.now() % 100000000)
        root.busy = true
        root.errorText = ""
        root.backend.send({c: "menuaction", op: "installed", id: root.requestId})
    }

    // The one reply this card asks for. The id is its own, so a menu action in flight behind the
    // panel answers its own dialog and never this one.
    Connections {
        target: root.backend
        function onMenuResult(message) {
            if (message.op !== "installed" || message.id !== root.requestId) return
            root.busy = false
            if (!message.ok) {
                root.errorText = message.error || "The installed applications could not be read."
                return
            }
            root.catalogue(message.installed || [])
        }
    }

    // The catalogue lands while the card is already up, so the cursor takes the rule's own row then
    // rather than on open: a card opened over an empty list has nothing to put it on yet.
    onInstalledChanged: root.cursor = root.cursorFor(root.editingApp)

    function cursorFor(id) {
        for (var i = 0; i < root.matches.length; i++) {
            if (root.matches[i].id === id) return i
        }
        return 0
    }

    function close() {
        if (!root.opened) return
        root.opened = false
        root.closed()
        if (root.focusHolder) root.focusHolder.forceActiveFocus()
    }

    function submit() {
        if (!root.canSubmit) return
        root.saved(root.ending, root.chosen.id)
        root.close()
    }

    function moveCursor(delta) {
        var last = root.matches.length - 1
        if (last < 0) return
        root.cursor = Math.max(0, Math.min(last, root.cursor + delta))
        list.positionViewAtIndex(root.cursor, ListView.Contain)
    }

    // The card's own Tab cycle, the shape ui/MenuActionDialog.qml stepFocus draws: the two fields,
    // the list, then the two buttons, and it wraps rather than leaving the card.
    function stepFocus(back) {
        var items = [field, search, listFocus, closeFocus, submitFocus].filter(function (item) {
            return item.visible && item.enabled && item.activeFocusOnTab
        })
        if (!items.length) return
        var current = -1
        for (var i = 0; i < items.length; i++) if (items[i].activeFocus) current = i
        var next = current < 0 ? (back ? items.length - 1 : 0)
            : (current + (back ? -1 : 1) + items.length) % items.length
        items[next].forceActiveFocus()
    }

    Keys.onTabPressed: function (event) { root.stepFocus((event.modifiers & Qt.ShiftModifier) !== 0); event.accepted = true }
    Keys.onBacktabPressed: function (event) { root.stepFocus(true); event.accepted = true }
    Keys.onEscapePressed: function (event) { root.close(); event.accepted = true }
    Keys.onUpPressed: function (event) { root.moveCursor(-1); event.accepted = true }
    Keys.onDownPressed: function (event) { root.moveCursor(1); event.accepted = true }
    Keys.onReturnPressed: function (event) { root.submit(); event.accepted = true }
    Keys.onEnterPressed: function (event) { root.submit(); event.accepted = true }
    Keys.onPressed: function (event) { event.accepted = true }

    Rectangle {
        anchors.fill: parent
        color: Theme.color.background
        opacity: 0.5
        MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            onClicked: root.close()
            onWheel: function (wheel) { wheel.accepted = true }
        }
    }

    Rectangle {
        id: card
        anchors.centerIn: parent
        width: Math.max(0, Math.min(Theme.space(420) * Theme.dialogWidthRatio, root.width - 2 * Theme.spacing.gap))
        height: Math.max(0, Math.min(body.implicitHeight + 2 * Theme.spacing.rowPaddingX, root.height - 2 * Theme.spacing.gap))
        color: Theme.color.surface
        border.color: Theme.color.muted
        border.width: Theme.spacing.hairline
        radius: Style.cornerRadius

        MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            onWheel: function (wheel) { wheel.accepted = true }
        }

        Column {
            id: body
            anchors.fill: parent
            anchors.margins: Theme.spacing.rowPaddingX
            spacing: Theme.spacing.gap

            Flea.DialogTitle {
                width: parent.width
                text: root.editing.length > 0 ? "Open " + root.editing + " with" : "Open an ending with"
            }

            Rectangle { width: parent.width; height: Theme.spacing.hairline; color: Theme.color.muted; opacity: 0.4 }

            Text {
                width: parent.width
                text: "File ending"
                textFormat: Text.PlainText
                color: Theme.color.foreground
                font { family: Theme.font.family; pixelSize: Theme.font.body }
            }

            Rectangle {
                width: parent.width
                height: Theme.rowHeight
                color: Theme.color.background
                border.color: field.activeFocus ? Theme.color.accent : Theme.color.muted
                border.width: Theme.spacing.hairline
                TextInput {
                    id: field
                    anchors.fill: parent
                    anchors.margins: Theme.spacing.gap
                    color: root.endingValid || field.text.length === 0 ? Theme.color.foreground : Theme.color.error
                    selectionColor: Theme.color.accent
                    selectedTextColor: Theme.color.background
                    font { family: Theme.font.family; pixelSize: Theme.font.body }
                    activeFocusOnTab: true
                    clip: true
                    selectByMouse: true
                    Accessible.name: "File ending"
                    Keys.onTabPressed: function (event) { root.stepFocus((event.modifiers & Qt.ShiftModifier) !== 0) }
                    Keys.onBacktabPressed: root.stepFocus(true)
                    Keys.onReturnPressed: root.submit()
                    Keys.onEnterPressed: root.submit()
                }
            }

            // One line, and it is the whole rule: lowercase, a leading dot, and the ending as the
            // file wears it, which is why .nii.gz is one ending here and not a .gz with a name.
            Text {
                width: parent.width
                text: root.errorText.length > 0 ? root.errorText
                      : field.text.length > 0 && !root.endingValid
                        ? "An ending is a dot and lowercase name parts, like .nii.gz"
                        : "Written as the file wears it: .nii, .nii.gz, .mha.gz"
                color: root.errorText.length > 0 || (field.text.length > 0 && !root.endingValid)
                       ? Theme.color.error : Theme.color.muted
                font { family: Theme.font.family; pixelSize: Theme.font.caption }
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
            }

            Rectangle {
                width: parent.width
                height: Theme.rowHeight
                color: Theme.color.background
                border.color: search.activeFocus ? Theme.color.accent : Theme.color.muted
                border.width: Theme.spacing.hairline
                TextInput {
                    id: search
                    anchors.fill: parent
                    anchors.margins: Theme.spacing.gap
                    color: Theme.color.foreground
                    selectionColor: Theme.color.accent
                    selectedTextColor: Theme.color.background
                    font { family: Theme.font.family; pixelSize: Theme.font.body }
                    activeFocusOnTab: true
                    clip: true
                    selectByMouse: true
                    Accessible.name: "Search applications"
                    onTextChanged: root.cursor = 0
                    Keys.onTabPressed: function (event) { root.stepFocus((event.modifiers & Qt.ShiftModifier) !== 0) }
                    Keys.onBacktabPressed: root.stepFocus(true)
                    Keys.onReturnPressed: root.submit()
                    Keys.onEnterPressed: root.submit()
                    Text {
                        anchors.fill: parent
                        visible: search.text.length === 0 && !search.activeFocus
                        text: "Search applications"
                        color: Theme.color.muted
                        font { family: Theme.font.family; pixelSize: Theme.font.body }
                        textFormat: Text.PlainText
                        verticalAlignment: Text.AlignVCenter
                    }
                }
            }

            FocusScope {
                id: listFocus
                width: parent.width
                height: root.viewportRows * Theme.rowHeight
                activeFocusOnTab: true
                Keys.onTabPressed: function (event) { root.stepFocus((event.modifiers & Qt.ShiftModifier) !== 0) }
                Keys.onBacktabPressed: root.stepFocus(true)

                ListView {
                    id: list
                    anchors.fill: parent
                    clip: true
                    model: root.matches
                    reuseItems: true
                    boundsBehavior: Flickable.StopAtBounds
                    activeFocusOnTab: false

                    delegate: Flea.MenuRow {
                        required property var modelData
                        required property int index
                        width: list.width
                        entry: ({ label: modelData.label, action: "", icon: modelData.icon, glyph: "app-window", hint: "" })
                        current: root.cursor === index
                        onActivated: { root.cursor = index; root.submit() }
                    }
                }

                Flea.Spinner {
                    anchors.centerIn: parent
                    visible: root.busy
                }

                Text {
                    anchors.centerIn: parent
                    width: parent.width - 4 * Theme.spacing.rowPaddingX
                    visible: !root.busy && root.matches.length === 0 && search.text.trim().length > 0
                    text: OpenWith.noMatch(search.text)
                    color: Theme.color.muted
                    font { family: Theme.font.family; pixelSize: Theme.font.caption }
                    textFormat: Text.PlainText
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.Wrap
                }
            }

            Row {
                anchors.right: parent.right
                spacing: Theme.spacing.gap

                FocusScope {
                    id: closeFocus
                    width: closeButton.implicitWidth
                    height: closeButton.implicitHeight
                    activeFocusOnTab: true
                    Keys.onTabPressed: function (event) { root.stepFocus((event.modifiers & Qt.ShiftModifier) !== 0) }
                    Keys.onBacktabPressed: root.stepFocus(true)
                    Keys.onReturnPressed: root.close()
                    Keys.onSpacePressed: root.close()
                    Flea.DialogButton { id: closeButton; label: "Cancel"; primary: parent.activeFocus; onActivated: root.close() }
                }

                FocusScope {
                    id: submitFocus
                    width: submitButton.implicitWidth
                    height: submitButton.implicitHeight
                    activeFocusOnTab: root.canSubmit
                    Keys.onTabPressed: function (event) { root.stepFocus((event.modifiers & Qt.ShiftModifier) !== 0) }
                    Keys.onBacktabPressed: root.stepFocus(true)
                    Keys.onReturnPressed: root.submit()
                    Keys.onSpacePressed: root.submit()
                    Flea.DialogButton { id: submitButton; label: "Save"; primary: parent.activeFocus; available: root.canSubmit; onActivated: root.submit() }
                }
            }
        }
    }
}
