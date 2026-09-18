.pragma library

// Generated from keys.toml by tools/flea-keymap-gen. Do not edit.
var PRESETS = ["default"]
var preset = "default"
var PRESET_KEYS = [
    {"mods":"ctrl","key":"Insert","keys":"ctrl-insert","action":"copy","label":"copy","frontend":"tui","context":"listing","preset":"all","code":"Key_Insert","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Insert,"mask":(Qt.ControlModifier)},
    {"mods":"shift","key":"Insert","keys":"shift-insert","action":"paste","label":"paste","frontend":"tui","context":"listing","preset":"all","code":"Key_Insert","text":"","ctrl":false,"shift":true,"alt":false,"super":false,"keycode":Qt.Key_Insert,"mask":(Qt.ShiftModifier)},
    {"mods":"none","key":"Escape","keys":"escape","action":"escape","label":"escape","frontend":"all","context":"rail,menu,panel,preview,pdf,media","preset":"all","code":"Key_Escape","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Escape,"mask":(0)},
    {"mods":"none","key":"Menu","keys":"menu","action":"menu","label":"context menu","frontend":"all","context":"listing,rail","preset":"all","code":"Key_Menu","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Menu,"mask":(0)},
    {"mods":"shift","key":"F10","keys":"shift-f10","action":"menu","label":"context menu","frontend":"all","context":"listing,rail","preset":"all","code":"Key_F10","text":"","ctrl":false,"shift":true,"alt":false,"super":false,"keycode":Qt.Key_F10,"mask":(Qt.ShiftModifier)},
    {"mods":"none","key":"Down","keys":"down","action":"cursorDown","label":"cursorDown","frontend":"all","context":"rail,menu,panel,preview,pdf,media","preset":"all","code":"Key_Down","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Down,"mask":(0)},
    {"mods":"none","key":"Up","keys":"up","action":"cursorUp","label":"cursorUp","frontend":"all","context":"rail,menu,panel,preview,pdf,media","preset":"all","code":"Key_Up","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Up,"mask":(0)},
    {"mods":"text","key":"m","keys":"m","action":"mute","label":"mute","frontend":"all","context":"media","preset":"all","code":"","text":"m","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"none","key":"Return","keys":"return","action":"open","label":"open","frontend":"all","context":"rail,menu,panel,preview,pdf,media","preset":"all","code":"Key_Return","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Return,"mask":(0)},
    {"mods":"none","key":"Enter","keys":"enter","action":"open","label":"open","frontend":"all","context":"rail,menu,panel,preview,pdf,media","preset":"all","code":"Key_Enter","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Enter,"mask":(0)},
    {"mods":"none","key":"Space","keys":"space","action":"preview","label":"preview","frontend":"all","context":"rail,menu,panel,preview,pdf,media","preset":"all","code":"Key_Space","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Space,"mask":(0)},
    {"mods":"none","key":"Tab","keys":"tab","action":"focusNext","label":"focusNext","frontend":"all","context":"rail,menu,panel,preview,pdf,media","preset":"all","code":"Key_Tab","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Tab,"mask":(0)},
    {"mods":"shift","key":"Tab","keys":"shift-tab","action":"focusPrevious","label":"focusPrevious","frontend":"all","context":"rail,menu,panel,preview,pdf,media","preset":"all","code":"Key_Tab","text":"","ctrl":false,"shift":true,"alt":false,"super":false,"keycode":Qt.Key_Tab,"mask":(Qt.ShiftModifier)},
    {"mods":"shift","key":"Backtab","keys":"shift-backtab","action":"focusPrevious","label":"focusPrevious","frontend":"all","context":"rail,menu,panel,preview,pdf,media","preset":"all","code":"Key_Backtab","text":"","ctrl":false,"shift":true,"alt":false,"super":false,"keycode":Qt.Key_Backtab,"mask":(Qt.ShiftModifier)},
    {"mods":"none","key":"Right","keys":"right","action":"menuRight","label":"menuRight","frontend":"all","context":"menu","preset":"all","code":"Key_Right","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Right,"mask":(0)},
    {"mods":"none","key":"Left","keys":"left","action":"parent","label":"parent","frontend":"all","context":"menu","preset":"all","code":"Key_Left","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Left,"mask":(0)},
    {"mods":"none","key":"Left","keys":"left","action":"previewBack","label":"leave the preview, or back in it","frontend":"all","context":"preview,pdf,media","preset":"all","code":"Key_Left","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Left,"mask":(0)},
    {"mods":"none","key":"Right","keys":"right","action":"previewForward","label":"fill the window, or forward in it","frontend":"all","context":"preview,pdf,media","preset":"all","code":"Key_Right","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Right,"mask":(0)},
    {"mods":"none","key":"Escape","keys":"escape","action":"escape","label":"close editor","frontend":"all","context":"editor","preset":"all","code":"Key_Escape","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Escape,"mask":(0)},
]
var SHARED_KEYS = [
    {"mods":"ctrl","key":"C","keys":"ctrl-c","action":"copy","context":"listing","frontend":"all","preset":"all","code":"Key_C","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_C,"mask":(Qt.ControlModifier)},
    {"mods":"ctrl","key":"V","keys":"ctrl-v","action":"paste","context":"listing","frontend":"all","preset":"all","code":"Key_V","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_V,"mask":(Qt.ControlModifier)},
    {"mods":"ctrl","key":"X","keys":"ctrl-x","action":"cut","context":"listing","frontend":"all","preset":"all","code":"Key_X","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_X,"mask":(Qt.ControlModifier)},
    {"mods":"ctrl","key":"Z","keys":"ctrl-z","action":"undo","context":"listing","frontend":"all","preset":"all","code":"Key_Z","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Z,"mask":(Qt.ControlModifier)},
    {"mods":"ctrl","key":"F","keys":"ctrl-f","action":"search","context":"listing","frontend":"all","preset":"all","code":"Key_F","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_F,"mask":(Qt.ControlModifier)},
    {"mods":"ctrl","key":"B","keys":"ctrl-b","action":"copypath","context":"listing","frontend":"all","preset":"all","code":"Key_B","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_B,"mask":(Qt.ControlModifier)},
    {"mods":"ctrl","key":"G","keys":"ctrl-g","action":"sidebar","context":"listing","frontend":"all","preset":"all","code":"Key_G","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_G,"mask":(Qt.ControlModifier)},
    {"mods":"ctrl","key":"T","keys":"ctrl-t","action":"toggleDual","context":"listing","frontend":"all","preset":"all","code":"Key_T","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_T,"mask":(Qt.ControlModifier)},
    {"mods":"ctrl","key":"N","keys":"ctrl-n","action":"newFolder","context":"listing","frontend":"all","preset":"all","code":"Key_N","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_N,"mask":(Qt.ControlModifier)},
    {"mods":"ctrl","key":"M","keys":"ctrl-m","action":"newFile","context":"listing","frontend":"all","preset":"all","code":"Key_M","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_M,"mask":(Qt.ControlModifier)},
    {"mods":"ctrl","key":"A","keys":"ctrl-a","action":"selectAll","context":"listing","frontend":"all","preset":"all","code":"Key_A","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_A,"mask":(Qt.ControlModifier)},
    {"mods":"ctrl","key":"H","keys":"ctrl-h","action":"toggleHidden","context":"listing","frontend":"all","preset":"all","code":"Key_H","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_H,"mask":(Qt.ControlModifier)},
    {"mods":"none","key":"Down","keys":"down","action":"cursorDown","context":"listing","frontend":"all","preset":"all","code":"Key_Down","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Down,"mask":(0)},
    {"mods":"none","key":"Up","keys":"up","action":"cursorUp","context":"listing","frontend":"all","preset":"all","code":"Key_Up","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Up,"mask":(0)},
    {"mods":"none","key":"Left","keys":"left","action":"parent","context":"listing","frontend":"all","preset":"all","code":"Key_Left","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Left,"mask":(0)},
    {"mods":"none","key":"Right","keys":"right","action":"pageForward","context":"listing","frontend":"all","preset":"all","code":"Key_Right","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Right,"mask":(0)},
    {"mods":"none","key":"Return","keys":"enter","action":"open","context":"listing","frontend":"all","preset":"all","code":"Key_Return","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Return,"mask":(0)},
    {"mods":"none","key":"Enter","keys":"enter","action":"open","context":"listing","frontend":"all","preset":"all","code":"Key_Enter","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Enter,"mask":(0)},
    {"mods":"none","key":"Delete","keys":"delete","action":"trash","context":"listing","frontend":"all","preset":"all","code":"Key_Delete","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Delete,"mask":(0)},
    {"mods":"none","key":"Escape","keys":"escape","action":"escape","context":"listing","frontend":"all","preset":"all","code":"Key_Escape","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Escape,"mask":(0)},
    {"mods":"none","key":"Tab","keys":"tab","action":"focusNext","context":"listing","frontend":"all","preset":"all","code":"Key_Tab","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Tab,"mask":(0)},
    {"mods":"none","key":"Space","keys":"space","action":"toggleSelect","context":"listing","frontend":"all","preset":"all","code":"Key_Space","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Space,"mask":(0)},
    {"mods":"none","key":"F2","keys":"f2","action":"rename","context":"listing","frontend":"all","preset":"all","code":"Key_F2","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_F2,"mask":(0)},
    {"mods":"text","key":"m","keys":"m","action":"menu","context":"listing","frontend":"all","preset":"all","code":"","text":"m","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":",","keys":",","action":"settings","context":"listing","frontend":"all","preset":"all","code":"","text":",","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":"?","keys":"?","action":"keymapSheet","context":"listing","frontend":"all","preset":"all","code":"","text":"?","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
]
var POINTER = [{"where":"listing","press":"left","row":"any","does":"selectOnly","label":"put the cursor on the row and drop any other selection"},{"where":"listing","press":"left x2","row":"any","does":"open","label":"open the row"},{"where":"listing","press":"left","row":"result","does":"reveal","label":"go to the file in its own directory, selected"},{"where":"listing","press":"left x2","row":"result","does":"reveal","label":"still the one reveal the first tap made"},{"where":"listing","press":"ctrl left","row":"any","does":"toggleSelect","label":"add the row to the selection"},{"where":"listing","press":"shift left","row":"any","does":"extendSelect","label":"extend the selection to the row"},{"where":"listing","press":"ctrl left x2","row":"any","does":"toggleSelect","label":"still only selects"},{"where":"listing","press":"shift left x2","row":"any","does":"extendSelect","label":"still only selects"},{"where":"listing","press":"left","row":"renaming","does":"commitRename","label":"commit the open rename, then select the row"},{"where":"listing","press":"right","row":"any","does":"menu","label":"open the context menu at the pointer, on the selection the row is in"},{"where":"column","press":"left","row":"dir","does":"open","label":"go into that directory on one tap, as its neighbours do"},{"where":"neighbour","press":"left","row":"dir","does":"reveal","label":"show that directory in the middle column"},{"where":"neighbour","press":"left","row":"file","does":"nothing","label":"a file has no contents to reveal"},{"where":"neighbour","press":"left x2","row":"file","does":"open","label":"open the file"},{"where":"neighbour","press":"right","row":"any","does":"nothing","label":"a peeked row has no menu"},{"where":"chrome","press":"left","row":"parent","does":"goToCrumb","label":"open the directory that segment of the path names"},{"where":"chrome","press":"left x2","row":"any","does":"pathBar","label":"type the path instead of clicking it"},{"where":"window","press":"back","row":"any","does":"backOrParent","label":"go back through the history, or up a directory when there is none"},{"where":"rail","press":"left","row":"any","does":"open","label":"open the place"},{"where":"rail","press":"right","row":"any","does":"menu","label":"eject and unmount"}]
var BASE_SHEET = [{"keys":"down up","action":"cursorDown","label":"move"},{"keys":"enter","action":"open","label":"open"},{"keys":"left","action":"parent","label":"parent"},{"keys":"right","action":"pageForward","label":"open / preview"},{"keys":"space","action":"toggleSelect","label":"select"},{"keys":"^a","action":"selectAll","label":"select all"},{"keys":"^f","action":"search","label":"find"},{"keys":"tab","action":"focusNext","label":"scope / focus"},{"keys":"^g","action":"sidebar","label":"sidebar"},{"keys":"^c","action":"copy","label":"copy"},{"keys":"^b","action":"copypath","label":"file path"},{"keys":"^x","action":"cut","label":"cut"},{"keys":"^v","action":"paste","label":"paste"},{"keys":"f2","action":"rename","label":"rename"},{"keys":"delete","action":"trash","label":"trash"},{"keys":"^z","action":"undo","label":"undo"},{"keys":"^N","action":"newFolder","label":"new folder"},{"keys":"m","action":"menu","label":"menu"},{"keys":",","action":"settings","label":"settings"},{"keys":"?","action":"keymapSheet","label":"keys"}]
var SHEET_GROUPS = {"move":["cursorDown","cursorUp","open","parent","pageForward","focusNext","escape","toggleSelect","selectAll"],"look":["preview","sidebar","toggleDual","toggleHidden","mute","keymapSheet"],"find":["search"],"change":["copy","cut","paste","copypath","rename","trash","undo","newFolder","newFile","menu","settings"]}
var SHEET_EXTRA = [{"preset":"all","mods":"text","keys":"m","action":"mute","label":"mute"},{"preset":"all","mods":"none","keys":"space","action":"preview","label":"preview"}]
var DIGITS = {"from":1,"to":9,"prefix":"tab"}

function applies(row, context, frontend) {
    return (row.context.split(",").indexOf(context) >= 0 || row.context === "all")
           && (row.frontend === frontend || row.frontend === "all")
}
function matches(row, key, text, modifiers) {
    var mask = modifiers & (Qt.ControlModifier | Qt.ShiftModifier | Qt.AltModifier | Qt.MetaModifier)
    if (row.mods === "text")
        return (mask & ~Qt.ShiftModifier) === 0 && text === row.text
    return mask === row.mask && key === row.keycode
}
// A matching empty action suppresses fallback, as Mac Ctrl+X requires.
function presetMatch(name, key, text, modifiers, context, frontend) {
    for (var pass = 0; pass < 2; pass++) {
        for (var i = 0; i < PRESET_KEYS.length; i++) {
            var row = PRESET_KEYS[i]
            if (row.preset !== (pass === 0 ? name : "all")) continue
            if (applies(row, context, frontend) && matches(row, key, text, modifiers)) return row
        }
    }
    return null
}
function lookupFor(name, key, text, modifiers, context, frontend) {
    context = context || "listing"
    frontend = frontend || "gui"
    var row = presetMatch(name, key, text, modifiers, context, frontend)
    if (row) return row.action
    if (context !== "listing" && context !== "rail") return ""
    for (var i = 0; i < SHARED_KEYS.length; i++) {
        if (matches(SHARED_KEYS[i], key, text, modifiers)) return SHARED_KEYS[i].action
    }
    if (frontend === "tui" && modifiers === 0 && text >= String(DIGITS.from) && text <= String(DIGITS.to))
        return DIGITS.prefix + text
    return ""
}
function lookup(key, text, modifiers, context, frontend) {
    return lookupFor(preset, key, text, modifiers, context, frontend)
}
function actionGroup(action) {
    var arms = { copyArm: "copy", cutArm: "cut", pasteArm: "paste", cursorFirstArm: "cursorFirst", trashArm: "trash" }
    return arms[action] || action
}
function bindingRows(name, frontend) {
    var rows = [], candidates = PRESET_KEYS.concat(SHARED_KEYS)
    for (var i = 0; i < candidates.length; i++) {
        var row = candidates[i]
        if (row.preset !== "all" && row.preset !== name) continue
        if (!applies(row, "listing", frontend || "gui") || !row.action) continue
        if (lookupFor(name, row.keycode, row.text, row.mask, "listing", frontend || "gui") !== row.action) continue
        var duplicate = rows.some(function (kept) { return kept.keys === row.keys && kept.action === row.action })
        if (!duplicate) rows.push(row)
    }
    return rows
}
function hintFor(action) {
    return HINTS[action] || ""
}
// How wide a cap may get before a second spelling stops earning its place. The sheet draws two
// columns of a 300 unit card, so a cap past this elides and the wording beside it has nowhere to go.
var SHEET_CAP_BUDGET = 16

// An action id is not wording. A row the base sheet does not name printed its own identifier, so the
// pane advertised "pageDown" and "textSizeReset" beside sentences like "hidden files".
function spelledOut(action) {
    return String(action).replace(/([a-z0-9])([A-Z])/g, "$1 $2").toLowerCase()
}

// Which spelling speaks for an action: the preset's own before an inherited one, and a plain key
// before a chord. setPreset ranks the menu hint the same way, so the sheet and the menus agree.
function capRank(row, preset) {
    return (row.preset === preset ? 0 : 2) + (row.mods === "text" ? 0 : 1)
}

function sheetFor(name, frontend, dual) {
    var result = [], groups = {}
    var rows = bindingRows(name, frontend || "gui").concat(SHEET_EXTRA)
    for (var i = 0; i < rows.length; i++) {
        var row = rows[i], action = actionGroup(row.action)
        var group = groups[action]
        if (!group) {
            var label = row.label || spelledOut(action)
            for (var j = 0; j < BASE_SHEET.length; j++)
                if (actionGroup(BASE_SHEET[j].action) === action) label = BASE_SHEET[j].label
            group = { action: action, label: label, keys: "", context: row.context || "listing", spellings: [] }
            result.push(group)
            groups[action] = group
        }
        group.spellings.push(row)
    }
    // One cap names one key. Joining every spelling an action answers to built caps of 40 characters
    // on the default preset and 78 on mac, wider than the whole card, so the pane drew them across
    // the column beside it. The best spelling always shows, a second only while both still fit.
    for (var g = 0; g < result.length; g++) {
        var spellings = result[g].spellings.slice()
        spellings.sort(function (left, right) { return capRank(left, name) - capRank(right, name) })
        var keys = spellings.length ? spellings[0].keys : ""
        for (var k = 1; k < spellings.length; k++) {
            var both = keys + " / " + spellings[k].keys
            if (spellings[k].keys === keys || both.length > SHEET_CAP_BUDGET) continue
            keys = both
            break
        }
        result[g].keys = keys
        delete result[g].spellings
    }
    if (dual && groups.focusNext) {
        groups.focusNext.keys = "tab"
        groups.focusNext.label = "focus other pane"
    }
    return result
}
function setPreset(name) {
    preset = PRESETS.indexOf(name) >= 0 ? name : "default"
    SHEET = sheetFor(preset, "gui")
    HINTS = {}
    var ranks = {}, rows = bindingRows(preset, "gui")
    for (var i = 0; i < rows.length; i++) {
        var row = rows[i]
        if (row.mods !== "text" && row.mods !== "none" && row.mods !== "ctrl" && row.mods !== "ctrlshift") continue
        var action = actionGroup(row.action), rank = (row.preset === preset ? 0 : 2) + (row.mods === "text" ? 0 : 1)
        if (ranks[action] !== undefined && ranks[action] <= rank) continue
        // A menu hint is the key the operator presses: Menus.html and the OpenWith overseer board
        // both draw Move to Trash with its own key. Ctrl chords count, because they are what most
        // of these actions answer to now that the bare letters have left the table; without them
        // every menu row but Trash drew no hint at all.
        HINTS[action] = row.mods === "text" ? row.key : row.keys
        ranks[action] = rank
    }
}
var SHEET = []; var HINTS = {}
setPreset("default")
