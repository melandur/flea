import QtQuick
import "js/Menu.js" as Menu
import "js/LocalSend.js" as LocalSendJs

// Directive 71: the menu's LocalSend row is the Taildrop row's twin, so this is ui/Taildrop.qml's
// shape for the other one: the only thing here that knows about LocalSend, with ui/ContextMenu.qml
// reading "peers" and rendering it. The work itself is the backend's, which drives localsend-cli on
// a pty of its own; nothing here ever opens the LocalSend app.
Item {
    id: root

    property var backend: null
    // [{id, label}], the devices the CLI discovered, named the way their own owners named them.
    property var peers: []
    property string reason: "checking"
    property bool checking: false
    signal refreshed()
    signal completed(bool ok, string reason)

    // The list is asked for when a menu opens, and kept while it is warm: discovery costs a CLI run
    // of over a second, and a right click is not a reason to announce this box on the LAN again.
    readonly property int warmMs: 15000
    property double _askedAt: 0

    function refresh(installed) {
        if (!installed) {
            root.peers = []
            root.reason = "localsend-cli is not installed"
            return true
        }
        // Discovery starts a CLI run that announces this box and receives for its length, so a menu
        // with the row hidden in Settings never starts one.
        if (Menu.isHidden(ViewState.menuHidden, "localsend")) {
            root.peers = []
            root.reason = "hidden in Settings"
            return true
        }
        if (root.checking) return false
        if (root._askedAt > 0 && Date.now() - root._askedAt < root.warmMs) return true
        root.checking = true
        root.backend.localSend("peers", "", [])
        return true
    }

    function send(peer, paths) {
        if (!peer || paths.length === 0) return false
        root.backend.localSend("send", peer, paths)
        return true
    }

    function answered(list, why) {
        root._askedAt = Date.now()
        root.checking = false
        var rows = []
        var named = {}
        for (var n = 0; n < list.length; n++) named[list[n].name] = (named[list[n].name] || 0) + 1
        // Two devices under one name are told apart by their address, which is also what the send checks.
        for (var i = 0; i < list.length; i++)
            rows.push({ id: LocalSendJs.peerId(list[i].name, list[i].address),
                        label: String(list[i].name) + (named[list[i].name] > 1 ? " (" + list[i].address + ")" : "") })
        root.peers = rows
        root.reason = why
        root.refreshed()
    }
}
