#!/bin/bash
# The iced window's keyboard, driven for real without the operator's screen or focus. A private sway
# runs with the headless backend on a socket of its own, never as a session; the iced window, wtype
# and grim are all pointed at that socket alone, so the desktop never sees a window and never loses
# its focus. Every step is checked against the request trace (FLEA_ICED_TRACE), the disk, the state
# file or the chooser's reply file, never against what a screenshot looks like. Screenshots are
# kept beside the fixture for a person to look at. docs/iced-port.md stage 7 records why each odd
# thing below is here; each was found by this run failing without it.
set -u
# Hard rule 9's guard, which owns FIXTURE_ROOT and every create and delete below.
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1
REPO=$PWD

BIN=$REPO/target/debug/flea
ICED=$REPO/crates/flea-iced/target/debug/flea-iced
POINTER=$REPO/crates/flea-iced/target/debug/examples/test-pointer
[ -x "$BIN" ] || { echo "iced-headless.sh: $BIN is missing, run cargo build" >&2; exit 1; }
[ -x "$ICED" ] || { echo "iced-headless.sh: $ICED is missing, run cargo build in crates/flea-iced" >&2; exit 1; }
[ -x "$POINTER" ] || { echo "iced-headless.sh: $POINTER is missing, run cargo build --example test-pointer in crates/flea-iced" >&2; exit 1; }
for tool in sway swaymsg wtype grim jq wl-paste magick gst-launch-1.0; do
    command -v "$tool" > /dev/null || { echo "iced-headless.sh: $tool is not installed" >&2; exit 1; }
done
[ -n "${XDG_RUNTIME_DIR:-}" ] || { echo "iced-headless.sh: XDG_RUNTIME_DIR is not set" >&2; exit 1; }

# One fixed name, so a run replaces the last one's fixture and screenshots rather than piling up.
D="$FIXTURE_ROOT/flea-iced-headless"
sandbox_make "$D"
T=$D/tree
mkdir -p "$T/alpha" "$T/beta" "$D/state" "$D/cache" "$D/data" "$D/shots" "$D/bin"
for i in $(seq -w 1 40); do echo "file $i" > "$T/note-$i.txt"; done
echo hidden > "$T/.secret"
echo inner > "$T/alpha/inside.txt"
# Run script's one script, in a config home of the suite's own: it writes the paths it was given.
mkdir -p "$D/config/flea/scripts"
printf '#!/bin/sh\nprintf "%%s\\n" "$@" > "%s/script.out"\n' "$D" > "$D/config/flea/scripts/mark.sh"
chmod +x "$D/config/flea/scripts/mark.sh"

pass=0
fail=0
check() {
    if [ "$2" = "$3" ]; then pass=$((pass + 1)); echo "ok   $1"
    else fail=$((fail + 1)); echo "FAIL $1: expected [$2] got [$3]"; fi
}

# Something on this box SIGKILLs any process named sway a moment after it starts, with nothing in
# its log; the same binary under another name runs. The copy is what makes the run possible here.
cp "$(command -v sway)" "$D/bin/flea-test-compositor"
cat > "$D/compositor.conf" <<'EOF'
output HEADLESS-1 resolution 1600x1000 position 0 0
default_border none
focus_follows_mouse no
xwayland disable
swaybg_command -
EOF

COMP=""
APP=""
cleanup() {
    [ -n "$APP" ] && kill "$APP" 2> /dev/null
    [ -n "$COMP" ] && kill "$COMP" 2> /dev/null
    [ -n "${BUS_PID:-}" ] && kill "$BUS_PID" 2> /dev/null
    sleep 0.5
    [ -n "$COMP" ] && rm -f "$XDG_RUNTIME_DIR/sway-ipc.$(id -u).$COMP.sock"
}
trap cleanup EXIT

before=$(ls "$XDG_RUNTIME_DIR" | grep -E '^wayland-[0-9]+$' | sort)
env -u WAYLAND_DISPLAY -u DISPLAY -u SWAYSOCK -u HYPRLAND_INSTANCE_SIGNATURE \
    WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 \
    setsid "$D/bin/flea-test-compositor" -c "$D/compositor.conf" > "$D/compositor.log" 2>&1 < /dev/null &
COMP=$!
sleep 2
after=$(ls "$XDG_RUNTIME_DIR" | grep -E '^wayland-[0-9]+$' | sort)
SOCK=$(comm -13 <(echo "$before") <(echo "$after") | head -1)
IPC=$XDG_RUNTIME_DIR/sway-ipc.$(id -u).$COMP.sock
kill -0 "$COMP" 2> /dev/null && [ -n "$SOCK" ] && [ -S "$IPC" ] \
    || { echo "iced-headless.sh: the headless compositor did not start, see $D/compositor.log" >&2; exit 1; }
comp() { WAYLAND_DISPLAY=$SOCK SWAYSOCK=$IPC swaymsg "$@"; }
shot() { WAYLAND_DISPLAY=$SOCK grim "$D/shots/$1.png"; }

# Every launch is pointed at the private socket, with sandboxed state, cache and data homes.
launch() {
    rm -f "$D/reply.json"
    env -u DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE WAYLAND_DISPLAY="$SOCK" \
        XDG_STATE_HOME="$D/state" XDG_CACHE_HOME="$D/cache" XDG_DATA_HOME="$D/data" XDG_CONFIG_HOME="$D/config" \
        FLEA_BIN="$BIN" FLEA_ICED_TRACE=1 FLEA_PICKER_REPLY="$D/reply.json" \
        DBUS_SESSION_BUS_ADDRESS="${TRASH_BUS:-${DBUS_SESSION_BUS_ADDRESS:-}}" \
        "$ICED" "$@" > "$D/out.log" 2> "$D/trace.log" &
    APP=$!
    sleep 3
}
pick() { FLEA_PICKER="$1" launch --pick; }
alive() { kill -0 "$APP" 2> /dev/null && echo 1 || echo 0; }
children() { pgrep -P "$APP" -c -x flea; }

# A key goes to the private compositor only, and only while its focused window is the expected one
# and that window's last listing is inside the fixture. The 300 ms wtype waits before typing is
# what lets a client in a keyboard-less compositor take the new virtual keyboard's keymap first;
# without it every key is dropped.
WANT=com.thisisgm.flea.iced
k() {
    local focused last
    focused=$(comp -t get_tree | jq -r '.. | objects | select(.focused == true) | .app_id // ""')
    [ "$focused" = "$WANT" ] || { echo "REFUSED: the focused window is '$focused'"; exit 3; }
    last=$(grep '"c":"list"' "$D/trace.log" | tail -1 | sed 's/.*"path":"\([^"]*\)".*/\1/')
    case "$last" in "$T"*) ;; *) echo "REFUSED: the window is listing '$last'"; exit 3 ;; esac
    WAYLAND_DISPLAY=$SOCK wtype -s 300 "$@"
    sleep 0.6
}
mark() { wc -l < "$D/trace.log"; }
since() { tail -n +$(( $1 + 1 )) "$D/trace.log"; }
# No Home key is bound in the Default preset, so the top is fifty Ups away.
top() { for _ in 1 2 3 4 5; do k -k Up -k Up -k Up -k Up -k Up -k Up -k Up -k Up -k Up -k Up; done; }
exists() { [ -e "$1" ] && echo yes || echo no; }
ctrl() { k -M ctrl -k "$1" -m ctrl; }

# Every dialog row reachable from the keyboard, except the three services that answer from outside the
# sandbox: LocalSend's discovery announces this box on the LAN, so its row stays hidden and never
# starts one. The shelf is switched on in the file itself, because the patch path runs
# `omarchy plugin enable` on the operator's real bar and a test may not.
XDG_STATE_HOME="$D/state" "$BIN" --ui-state '{"menu":{"hidden":["taildrop","localsend","dropbox","sharelink"]}}' > /dev/null || { echo "iced-headless.sh: the state seed was refused" >&2; exit 1; }
jq '.shelf.enabled = true' "$D/state/flea/ui.json" > "$D/ui.json.next" && mv "$D/ui.json.next" "$D/state/flea/ui.json"

# The window. Return is never pressed on a file here: it runs the desktop's handler, which opened a
# terminal inside the test compositor the first time.
launch "$T"
n=$(mark); k -k Down -k Down -k Down -k Up
check "cursor keys send no request" 0 "$(since "$n" | grep -c '"c":')"
k -k Up -k Up -k Up -k Down
n=$(mark); k -k Right; k -k Left
check "Right enters the folder under the cursor" 1 "$(since "$n" | grep -c '"path":"[^"]*/tree/beta"')"
check "Left comes back to the parent" 1 "$(since "$n" | grep -c '"path":"[^"]*/tree"}')"
shot listing
n=$(mark); ctrl h
check "Ctrl+H lists hidden" 1 "$(since "$n" | grep -c '"hidden":true')"
check "Ctrl+H is stored" true "$(jq -r .hidden "$D/state/flea/ui.json")"
ctrl h
check "Ctrl+H again is stored off" false "$(jq -r .hidden "$D/state/flea/ui.json")"
k -k Down; k -k F2; ctrl a; k renamed.txt; k -k Return; sleep 1
check "F2 renames on disk" "yes no" "$(exists "$T/renamed.txt") $(exists "$T/note-01.txt")"
ctrl n; sleep 1
check "Ctrl+N makes a folder" yes "$(exists "$T/New Folder")"
k -k Escape; n=$(mark); ctrl z; sleep 1
check "Escape closes the editor and Ctrl+Z undoes" "1 no" "$(since "$n" | grep -c '"c":"undo"') $(exists "$T/New Folder")"
# A field that never took focus lets Escape through uncaptured and passes with or without the fix,
# so text is typed into the editor first; Ctrl+N then makes a folder only once the editor closed.
top; k -k Down -k Down -k Down; k -k F2; k zzz; k -k Escape; n=$(mark); ctrl n; sleep 1
check "Escape in a focused editor closes it, so Ctrl+N is answered" 1 "$(since "$n" | grep -c '"c":"mkdir"')"
k -k Escape; ctrl z; sleep 1
check "and the probe folder is undone" no "$(exists "$T/New Folder")"
top; k -k Down -k Down; k -k space; ctrl c; top; k -k Right; ctrl v; sleep 1
check "Space, Ctrl+C and Ctrl+V copy into the folder" yes "$(exists "$T/alpha/note-02.txt")"
k -k Left
n=$(mark); ctrl f; k note-3; sleep 1
check "Ctrl+F searches what was typed" 1 "$(since "$n" | grep -c '"c":"search".*"query":"note-3"')"
shot search
k -k Escape; k -k Escape
n=$(mark); ctrl t; sleep 1
check "Ctrl+T lists the second pane" 1 "$(since "$n" | grep -c '"c":"list"')"
shot split
ctrl t; sleep 1
check "leaving the split quits its child" 1 "$(since "$n" | grep -c '"c":"quit"')"
check "one backend left" 1 "$(children)"
top; k -k Down -k Down -k Down; n=$(mark); k -k Right; sleep 1; shot preview
check "Right on a file previews without listing" 0 "$(since "$n" | grep -c '"c":"list"')"
k -k Left
k -k comma; sleep 1; shot settings
k -k Down -k Down -k Down -k Down -k Down -k Down -k Down -k Down; k -k Return; sleep 1
check "the panel's hidden switch is stored" true "$(jq -r .hidden "$D/state/flea/ui.json")"
k -k Return; sleep 1
check "and switched back" false "$(jq -r .hidden "$D/state/flea/ui.json")"
k -k Escape
# The context menu, raised from the keyboard. Every key goes to it while it is up, so Right over a
# folder must not enter it; Escape must close it, which Ctrl+N being answered proves.
top; k m; shot menu; n=$(mark); k -k Right; k -k Left; k -k Escape
check "an open menu keeps Right from entering the folder" 0 "$(since "$n" | grep -c '"c":"list"')"
n=$(mark); k m; k -k Escape; ctrl n; sleep 1
check "Escape closes the menu, so Ctrl+N is answered" 1 "$(since "$n" | grep -c '"c":"mkdir"')"
k -k Escape; ctrl z; sleep 1
# The file menu here is Open, Open with, Cut, Copy, Paste (the copy above is still held), then
# Duplicate: five Downs. Show hidden files is the last row, and Down holds at the end, so twenty
# Downs land on it whatever sits between.
top; k -k Down -k Down; k m; k -k Down -k Down -k Down -k Down -k Down; k -k Return; sleep 1
check "the menu's Duplicate copies the cursor row" yes "$(exists "$T/note-02 copy.txt")"
k m; for _ in $(seq 20); do k -k Down; done; k -k Return; sleep 1
check "the menu's last row shows hidden files" true "$(jq -r .hidden "$D/state/flea/ui.json")"
k m; for _ in $(seq 20); do k -k Down; done; k -k Return; sleep 1
check "and the same row, relabelled, hides them" false "$(jq -r .hidden "$D/state/flea/ui.json")"
top; ctrl b; sleep 1
check "Ctrl+B copies the cursor row's path" "$T/alpha" "$(WAYLAND_DISPLAY=$SOCK timeout 3 wl-paste -n 2> /dev/null)"
# The key sheet: ? opens it, and the key that closes it reaches nothing else, so Right over a folder
# behind it lists nothing; the next Right is the listing's again.
top; k '?'; sleep 0.5; shot sheet; n=$(mark); k -k Right
check "the key sheet takes the key that closes it" 0 "$(since "$n" | grep -c '"c":"list"')"
k -k Right; k -k Left
check "and the listing has the keyboard back" 2 "$(since "$n" | grep -c '"c":"list"')"
# The dialog rows: Compress, Add to shelf, then Trash and Delete permanently, then Open in terminal,
# Move to, Copy to and Properties, so Delete permanently is ten Downs and Properties fourteen.
top; k -k Down -k Down; n=$(mark); k m; for _ in $(seq 14); do k -k Down; done; k -k Return; sleep 1; shot properties
k -k Escape; sleep 0.5
check "Properties snapshots, asks, and Escape closes it" "1 1 1" "$(since "$n" | grep -c '"op":"snapshot"') $(since "$n" | grep -c '"op":"properties"') $(since "$n" | grep -c '"op":"close"')"
ctrl m; ctrl a; k made.txt; k -k Return; sleep 1
check "Ctrl+M's card creates the file" yes "$(exists "$T/made.txt")"
k m; for _ in $(seq 10); do k -k Down; done; k -k Return; sleep 1; shot delete; k -k Return; sleep 1
check "Enter on the confirmation's first focus cancels" yes "$(exists "$T/made.txt")"
k m; for _ in $(seq 10); do k -k Down; done; k -k Return; sleep 1; k -k Tab; k -k Return; sleep 1
check "Tab to Delete and Enter deletes it from disk" no "$(exists "$T/made.txt")"
# The job rows. Compress is seven Downs and Right opens its formats; the first one is taken. The new
# archive sorts straight after its source, and an archive row's Extract sits right under Compress.
top; k -k Down -k Down; n=$(mark); k m; for _ in $(seq 7); do k -k Down; done; k -k Right; shot compress; k -k Return; sleep 2
check "Compress snapshots, activates and writes an archive" "1 2" "$(since "$n" | grep -c '"op":"compress"') $(ls "$T" | grep -c '^note-02 copy\.')"
top; k -k Down -k Down -k Down; k m; for _ in $(seq 8); do k -k Down; done; k -k Return; sleep 2
check "Extract unpacks it beside itself" yes "$(exists "$T/note-02 copy/note-02 copy.txt")"
# Run script is the row after Copy path: seventeen Downs on a row whose archive group has no Extract.
top; k -k Down -k Down; k m; for _ in $(seq 17); do k -k Down; done; k -k Right; k -k Return; sleep 2
check "Run script runs it over the cursor row" "$T/note-02 copy" "$(cat "$D/script.out" 2> /dev/null)"
# Add to shelf is the share group's one row here, eight Downs: the cursor row goes on the sandbox's pile.
top; k -k Down -k Down; k m; for _ in $(seq 8); do k -k Down; done; shot shelf; k -k Return; sleep 2
check "Add to shelf puts the row on the pile" 1 "$(jq --arg p "$T/note-02 copy" '[.. | strings | select(. == $p)] | length' "$D/state/omarchy/flea-shelf/shelf.json" 2> /dev/null)"
# Open with reads the catalogue and launches nothing here: Escape closes it.
n=$(mark); k m; k -k Down; k -k Return; sleep 2; shot openwith; for _ in $(seq 10); do k -k Down; done; shot openwith-scrolled; k -k Escape; sleep 0.5
check "Open with reads the whole catalogue and Escape closes it" "1 1" "$(since "$n" | grep -c '"installed":true') $(since "$n" | grep -c '"op":"close"')"
k -k Tab; k -k Down -k Down; shot rail; k -k Tab
n=$(mark); backend=$(pgrep -P "$APP" -x flea)
comp "[app_id=\"$WANT\"] kill" > /dev/null; sleep 3
check "closing the window sends quit" 1 "$(since "$n" | grep -c '"c":"quit"')"
check "and the window and its backend both exit" "0 0" "$(alive) $(kill -0 "$backend" 2> /dev/null && echo 1 || echo 0)"

# The grid, from a window launched with view "grid". Which row a step lands on is asked of the backend
# itself, so the check never re-derives the listing's sort in shell.
nth_row() {
    printf '{"c":"list","path":"%s","first":60,"hidden":false}\n{"c":"quit"}\n' "$T" | "$BIN" --backend 2> /dev/null \
        | jq -r --argjson n "$1" 'select(.t == "rows") | .rows[$n].n'
}
jq '.view = "grid"' "$D/state/flea/ui.json" > "$D/ui.json.next" && mv "$D/ui.json.next" "$D/state/flea/ui.json"
WANT=com.thisisgm.flea.iced
launch "$T"; shot grid
n=$(mark); k -k Right; k -k Return; sleep 1
check "in the grid Right steps across to the next tile" 1 "$(since "$n" | grep -c "\"path\":\"$T/$(nth_row 1)\"")"
k -k Escape
comp "[app_id=\"$WANT\"] kill" > /dev/null; sleep 3
launch "$T"
# 1600 px less the rail's 192 holds nine tiles of 146, so Down is nine rows on. F2 names the row it is on.
want=$(nth_row 9); n=$(mark); k -k Down; k -k F2; k -k Escape
k -k F2; ctrl a; k gridpicked.txt; k -k Return; sleep 1
check "in the grid Down steps a whole tile row" "no yes" "$(exists "$T/$want") $(exists "$T/gridpicked.txt")"
comp "[app_id=\"$WANT\"] kill" > /dev/null; sleep 3

# The columns view: the parent and the cursor's folder are peeked, and a move onto another folder
# peeks that one; the middle column is the pane's own listing, so Down still moves its cursor.
jq '.view = "columns"' "$D/state/flea/ui.json" > "$D/ui.json.next" && mv "$D/ui.json.next" "$D/state/flea/ui.json"
launch "$T"; shot columns
check "the columns view peeks the parent and the cursor's folder" "1 1" "$(grep -c "\"c\":\"peek\".*\"path\":\"$D\"" "$D/trace.log") $(grep -c "\"c\":\"peek\".*\"path\":\"$T/alpha\"" "$D/trace.log")"
n=$(mark); k -k Down; sleep 0.5
check "and a move onto another folder peeks that one" 1 "$(since "$n" | grep -c "\"c\":\"peek\".*\"path\":\"$T/beta\"")"
# A file under the cursor is the preview column's: once the cursor settles, one meta for it under
# the column's own token, asking for a line count because the row is text.
# Folders list first and earlier steps leave some behind, so the first file is counted, not assumed.
n=$(mark); for _ in $(seq 2 "$(find "$T" -mindepth 1 -maxdepth 1 -type d ! -name '.*' | wc -l)"); do k -k Down; done
sleep 1; shot columns-preview
check "a settled file row asks for its meta with a line count" 1 "$(since "$n" | grep -c "\"c\":\"meta\".*\"text\":true.*\"token\":[1-9]")"
comp "[app_id=\"$WANT\"] kill" > /dev/null; sleep 3

# The column's two readers that own a resource: a PDF draws its own page, and a clip builds no player
# until play is pressed. The press is the compositor's own seat cursor, so it never leaves this sway.
# The play mark sits in the last third of 1408 px of pane past the 192 px rail, under a 16 by 10 frame.
mkdir -p "$T/media"
magick -size 400x300 xc:white -fill red -draw "rectangle 40,40 360,260" xc:white "$T/media/a-doc.pdf"
gst-launch-1.0 -q videotestsrc num-buffers=50 pattern=ball ! video/x-raw,width=320,height=240,framerate=25/1 \
    ! videoconvert ! jpegenc ! matroskamux ! filesink location="$T/media/b-clip.mkv" > /dev/null
printf '// one comment\nfn main() {\n    let x: u8 = 42; /* open\n    still */\n    println!("hi {}", x);\n}\n' > "$T/media/c-code.rs"
launch "$T/media"; sleep 1; shot columns-pdf
check "a PDF in the column builds no player" 0 "$(grep -c libgstplayback "/proc/$APP/maps")"
# The toolbar's next chevron, then maximize, which opens the overlay on the column's page; Escape.
WAYLAND_DISPLAY=$SOCK "$POINTER" 1600 1000 1346 347; sleep 0.5; shot columns-pdf-turned
WAYLAND_DISPLAY=$SOCK "$POINTER" 1600 1000 1447 347; sleep 1; shot columns-pdf-expanded
k -k Escape; sleep 0.5
k -k Down; sleep 1
check "nor does moving onto a clip" 0 "$(grep -c libgstplayback "/proc/$APP/maps")"
WAYLAND_DISPLAY=$SOCK "$POINTER" 1600 1000 1158 353
sleep 1.5; shot columns-play
check "a press of play builds one and plays" yes "$(grep -q libgstplayback "/proc/$APP/maps" && echo yes || echo no)"
# Source draws in Monokai: the column's first lines, then the overlay's editor over the same file.
k -k Down; sleep 1; shot columns-code
k -k Right; sleep 1; shot overlay-code; k -k Escape; sleep 0.5
comp "[app_id=\"$WANT\"] kill" > /dev/null; sleep 3
# The chooser below counts rows in the tree, so the folder goes with the window.
rm -rf "$T/media"

# The Trash view. trash:/// is gvfsd-trash's, which on the session bus is the operator's own Trash,
# so this window gets a private bus whose gvfsd is started with the sandbox's data home and serves
# only the sandbox Trash. gvfsd also lists .Trash-<uid> on other mounted drives, so nothing
# destructive is pressed unless that private listing holds exactly the file this suite trashed.
BUSOUT=$(env XDG_DATA_HOME="$D/data" dbus-daemon --session --fork --print-address=1 --print-pid=1)
TRASH_BUS=$(printf '%s\n' "$BUSOUT" | sed -n 1p); BUS_PID=$(printf '%s\n' "$BUSOUT" | sed -n 2p)
pgio() { DBUS_SESSION_BUS_ADDRESS="$TRASH_BUS" XDG_DATA_HOME="$D/data" gio "$@" 2> /dev/null; }
alone() { [ "$(pgio trash --list)" = "$(printf 'trash:///%s\t%s' "$1" "$T/trashbin/$1")" ] && echo yes || echo no; }
rail_trash() { WAYLAND_DISPLAY=$SOCK "$POINTER" 1600 1000 60 137; sleep 1.5; }
jq '.view = "list" | .places.trashCount = true' "$D/state/flea/ui.json" > "$D/ui.json.next" && mv "$D/ui.json.next" "$D/state/flea/ui.json"
mkdir -p "$T/trashbin"; echo one > "$T/trashbin/keep-me.txt"; echo two > "$T/trashbin/gone.txt"
launch "$T/trashbin"
k -k Delete; sleep 1
check "Delete moves the cursor row into the sandbox Trash" "yes no" "$(exists "$D/data/Trash/files/gone.txt") $(exists "$T/trashbin/gone.txt")"
if [ "$(alone gone.txt)" = yes ]; then
    rail_trash; shot trash
    check "the rail's Trash row opens the view, listing with recovery" 1 "$(grep -c '"op":"list","recover":true' "$D/trace.log")"
    k -k space; k -k Delete; sleep 1; shot trash-confirm
    k -k Return; sleep 1
    check "Enter on the card's Cancel deletes nothing" yes "$(exists "$D/data/Trash/files/gone.txt")"
    k -k Delete; sleep 1; k -k Right; k -k Return; sleep 1.5
    check "the destructive button, reached deliberately, deletes it from disk" no "$(exists "$D/data/Trash/files/gone.txt")"
    k -k Escape; sleep 0.5; k -k Delete; sleep 1; shot trash-badge
    if [ "$(alone keep-me.txt)" = yes ]; then
        rail_trash; k -k space; k m; k -k Return; sleep 1.5
        check "Restore from the selection's menu puts it back" "yes no" "$(exists "$T/trashbin/keep-me.txt") $(exists "$D/data/Trash/files/keep-me.txt")"
        k -k Escape; sleep 0.5
    else
        echo "skip  restore: the private Trash lists more than this suite's own file"
    fi
else
    echo "skip  the Trash view's destructive checks: the private Trash lists more than this suite's own file"
fi
comp "[app_id=\"$WANT\"] kill" > /dev/null; sleep 3

# The 30 day sweep, opt in: one item backdated past thirty days and one fresh. It is switched on
# only when the private Trash holds exactly those two, because it deletes without asking.
echo old > "$T/trashbin/old.txt"; echo new > "$T/trashbin/fresh.txt"
pgio trash "$T/trashbin/old.txt" "$T/trashbin/fresh.txt"
sed -i 's/^DeletionDate=.*/DeletionDate=2020-01-01T00:00:00/' "$D/data/Trash/info/old.txt.trashinfo" 2> /dev/null
two=$(printf 'trash:///fresh.txt\t%s\ntrash:///old.txt\t%s' "$T/trashbin/fresh.txt" "$T/trashbin/old.txt")
if [ "$(pgio trash --list | sort)" = "$two" ]; then
    jq '.trashAutoEmpty = true | .trashSweptOn = 0' "$D/state/flea/ui.json" > "$D/ui.json.next" && mv "$D/ui.json.next" "$D/state/flea/ui.json"
    launch "$T/trashbin"; sleep 4
    today=$(( $(date -d 'today 00:00' +%s) / 86400 ))
    check "the sweep deletes what is past thirty days and keeps the rest" "no yes" "$(exists "$D/data/Trash/files/old.txt") $(exists "$D/data/Trash/files/fresh.txt")"
    check "and records the day it ran" "$today" "$(jq -r '.trashSweptOn' "$D/state/flea/ui.json")"
    comp "[app_id=\"$WANT\"] kill" > /dev/null; sleep 3
    launch "$T/trashbin"; sleep 4
    check "a second launch the same day sweeps nothing" 0 "$(grep -c '"recover":false' "$D/trace.log")"
    comp "[app_id=\"$WANT\"] kill" > /dev/null; sleep 3
    jq '.trashAutoEmpty = false' "$D/state/flea/ui.json" > "$D/ui.json.next" && mv "$D/ui.json.next" "$D/state/flea/ui.json"
else
    echo "skip  the sweep: the private Trash does not hold exactly this suite's two files"
fi
kill "$BUS_PID" 2> /dev/null; BUS_PID=""; TRASH_BUS=""
rm -rf "$T/trashbin"
# Off again: past here the windows are on the session bus, whose count is the operator's own Trash.
jq '.places.trashCount = false' "$D/state/flea/ui.json" > "$D/ui.json.next" && mv "$D/ui.json.next" "$D/state/flea/ui.json"

# Tabs, opened from a favourite's Places menu (its Extras row is on here, menu.hidden being seeded
# without placeMenu): the strip appears, a press on the first tab lists its folder again, a press on
# the second's close mark closes it with nothing listed, and the same menu removes the favourite.
jq --arg p "$T/alpha" '.places.favourites = [{"label": "alpha", "path": $p}]' "$D/state/flea/ui.json" > "$D/ui.json.next" && mv "$D/ui.json.next" "$D/state/flea/ui.json"
launch "$T"
n=$(mark); k -k Tab -k Down -k Down; k m; k -k Down -k Return; sleep 1; shot tabs
check "the Places menu opens its folder in a new tab" 1 "$(since "$n" | grep -c "\"c\":\"list\".*\"path\":\"$T/alpha\"")"
n=$(mark); WAYLAND_DISPLAY=$SOCK "$POINTER" 1600 1000 100 55; sleep 1
check "a press on the first tab lists its folder again" 1 "$(since "$n" | grep -c "\"c\":\"list\".*\"path\":\"$T\"")"
n=$(mark); WAYLAND_DISPLAY=$SOCK "$POINTER" 1600 1000 370 55; sleep 1; shot tabs-closed
check "closing the other tab lists nothing" 0 "$(since "$n" | grep -c '"c":"list"')"
k m; k -k Down -k Down -k Down -k Down -k Return; sleep 1.5
check "Remove from Favorites drops the row's own record" 0 "$(jq '.places.favourites | length' "$D/state/flea/ui.json")"
comp "[app_id=\"$WANT\"] kill" > /dev/null; sleep 3

# Density and the list's columns: compact rows, and the header's right click showing Mode, saved
# as the whole shown set.
jq '.density = "compact"' "$D/state/flea/ui.json" > "$D/ui.json.next" && mv "$D/ui.json.next" "$D/state/flea/ui.json"
launch "$T"
WAYLAND_DISPLAY=$SOCK "$POINTER" 1600 1000 400 55 3; sleep 0.5; k -k Return; sleep 1.5; shot columns-mode
check "the header menu's Show Mode adds it to the stored columns" '["name","size","date","mode"]' "$(jq -c '.columns' "$D/state/flea/ui.json")"
check "the list asks the folder walk for its visible folders" yes "$(grep -q '"c":"dirsize"' "$D/trace.log" && echo yes || echo no)"
WAYLAND_DISPLAY=$SOCK "$POINTER" 1600 1000 400 55 3; sleep 0.5; k -k Down -k Down -k Return; sleep 1.5; shot columns-items
check "and Show Items adds the count beside it" '["name","size","date","mode","items"]' "$(jq -c '.columns' "$D/state/flea/ui.json")"
comp "[app_id=\"$WANT\"] kill" > /dev/null; sleep 3
jq '.density = "normal" | .columns = ["name","size","date"]' "$D/state/flea/ui.json" > "$D/ui.json.next" && mv "$D/ui.json.next" "$D/state/flea/ui.json"

# Text size: a stored stop draws the whole window at that size over the 14 px the metrics were taken
# at; it is looked at, not asserted, because nothing the trace carries measures it.
jq '.display.textSize = {"mode": 20}' "$D/state/flea/ui.json" > "$D/ui.json.next" && mv "$D/ui.json.next" "$D/state/flea/ui.json"
launch "$T"; shot text-size-20
check "a window at text size 20 opens and lists" 1 "$(grep -c "\"c\":\"list\".*\"path\":\"$T\"" "$D/trace.log")"
comp "[app_id=\"$WANT\"] kill" > /dev/null; sleep 3
jq '.display.textSize = {"mode": "system"}' "$D/state/flea/ui.json" > "$D/ui.json.next" && mv "$D/ui.json.next" "$D/state/flea/ui.json"

# flea --select, FLEA_SELECT across exec: the folder opens with the target under the cursor AND
# marked. Ctrl+B reads the cursor; Ctrl+C after a step off it takes the selection, so the paste's
# transfer naming the target is what only the mark explains.
FLEA_SELECT="$T/note-02.txt" launch "$T"; ctrl b; sleep 1
check "a reveal puts the cursor on its target" "$T/note-02.txt" "$(WAYLAND_DISPLAY=$SOCK timeout 3 wl-paste -n 2> /dev/null)"
n=$(mark); k -k Down; ctrl c; ctrl v; sleep 1
check "and marks it, so a step off it still copies the target alone" "1 1" "$(since "$n" | grep -c '"c":"transfer"') $(since "$n" | grep '"c":"transfer"' | grep -c "\"paths\":\[\"$T/note-02.txt\"\]")"
comp "[app_id=\"$WANT\"] kill" > /dev/null; sleep 3

# Held, Space paints (ui/js/Marks.js): one wtype holds it across two Downs with a pause either side
# longer than the compositor's repeat delay, so a repeat that toggled the resting row would show.
# Ctrl+C names the marked rows in its paths request; a Down after the release must add nothing.
launch "$T"; n=$(mark)
k -P space -s 1500 -k Down -k Down -s 1500 -p space; k -k Down; ctrl c; sleep 1
check "a held Space marks each row it reaches once, and its release ends the sweep" "[0,1,2]" "$(since "$n" | grep '"c":"paths"' | tail -1 | sed 's/^flea-iced: sent //' | jq -c .rows)"
n=$(mark); k -k Up -k Up; k -P space -k Down -p space; ctrl c; sleep 1
check "pressed on a marked row, the sweep unmarks" "[0]" "$(since "$n" | grep '"c":"paths"' | tail -1 | sed 's/^flea-iced: sent //' | jq -c .rows)"
comp "[app_id=\"$WANT\"] kill" > /dev/null; sleep 3

# The selection band (ui/SelectionBand.qml): six files, so the ground under them takes a press. The
# chrome and the header are a row each, so row r is drawn from 74 + 37r. A drag from the ground up
# and left into row 1 (a band with no width marks nothing, Marquee.cells) marks rows 1 to 5, read
# off Ctrl+C's paths request; a second band held over rows 3 to 5 and cancelled with Escape
# mid-drag puts those five marks back, where a release would leave three.
mkdir -p "$T/band"; for i in 0 1 2 3 4 5; do : > "$T/band/band-$i.txt"; done
band() { WAYLAND_DISPLAY=$SOCK "$POINTER" 1600 1000 800 700 1 600 "$@"; }
launch "$T/band"
band 130; sleep 0.5; shot band; n=$(mark); ctrl c; sleep 1
check "a band from the ground marks every row it crosses" "[1,2,3,4,5]" "$(since "$n" | grep '"c":"paths"' | tail -1 | sed 's/^flea-iced: sent //' | jq -c .rows)"
band 210 2500 & held=$!; sleep 1.6; k -k Escape; wait "$held"; n=$(mark); ctrl c; sleep 1
check "Escape mid-band puts back the marks it began from" "[1,2,3,4,5]" "$(since "$n" | grep '"c":"paths"' | tail -1 | sed 's/^flea-iced: sent //' | jq -c .rows)"
comp "[app_id=\"$WANT\"] kill" > /dev/null; sleep 3
# The grid's edge scroll. The tiles stretch to fill the width, so the grid's only ground is the
# empty trailing cell of its last tile row: 206 files in nine columns leave one. Sixty wheel notches
# pin that row to the list's bottom edge, a band starts in the empty cell and is dragged up past the top.
# Held two seconds it must reach further up than the same drag released at once, because the list
# scrolled under it.
for i in $(seq -w 0 199); do : > "$T/band/tile-$i.txt"; done
jq '.view = "grid"' "$D/state/flea/ui.json" > "$D/ui.json.next" && mv "$D/ui.json.next" "$D/state/flea/ui.json"
corner() { WAYLAND_DISPLAY=$SOCK "$POINTER" 1600 1000 1515 900 1 1000 15 "$@"; }
bottom() { WAYLAND_DISPLAY=$SOCK "$POINTER" 1600 1000 800 500 4 60; sleep 0.5; }
launch "$T/band"; bottom
corner; n=$(mark); ctrl c; sleep 1
still=$(since "$n" | grep '"c":"paths"' | tail -1 | sed 's/^flea-iced: sent //' | jq '.rows | min')
comp "[app_id=\"$WANT\"] kill" > /dev/null; sleep 3
launch "$T/band"; bottom
corner 2000; sleep 0.5; shot band-grid; n=$(mark); ctrl c; sleep 1
held=$(since "$n" | grep '"c":"paths"' | tail -1 | sed 's/^flea-iced: sent //' | jq '.rows | min')
check "a band held past the edge scrolls and marks further up" yes "$([ "${held:-999}" -lt "${still:-0}" ] 2> /dev/null && echo yes || echo "no: $still then $held")"
comp "[app_id=\"$WANT\"] kill" > /dev/null; sleep 3
jq '.view = "list"' "$D/state/flea/ui.json" > "$D/ui.json.next" && mv "$D/ui.json.next" "$D/state/flea/ui.json"
# The chooser below counts the tree's rows.
rm -rf "$T/band"
APP=""

# The chooser, the window tools/flea-portal opens through flea --pick.
WANT=com.thisisgm.flea.picker
pick "{\"title\":\"Live pick\",\"multiple\":true,\"folder\":\"$T\",\"filters\":[{\"label\":\"Text\",\"globs\":[\"*.txt\"],\"mimes\":[]}]}"
k -k Down -k Down -k Down; k -k space; k -k Down; k -k space; sleep 1; shot chooser
k -k Return; sleep 3
check "the chooser answers both marks" 2 "$(jq -r '.uris | length' "$D/reply.json" 2> /dev/null)"
check "and exits" 0 "$(alive)"
pick "{\"folder\":\"$T\"}"
comp "[app_id=\"$WANT\"] kill" > /dev/null; sleep 3
check "a closed chooser answers a refusal" '{"response":1}' "$(cat "$D/reply.json" 2> /dev/null)"
pick "{\"mode\":\"save\",\"name\":\"renamed.txt\",\"folder\":\"$T\"}"
k -k Tab; ctrl a; k brand-new.txt; k -k Return; sleep 3
check "a typed save name is answered" "file://$T/brand-new.txt" "$(jq -r '.uris[0]' "$D/reply.json" 2> /dev/null)"
pick "{\"mode\":\"save\",\"name\":\"x.txt\",\"folder\":\"$T\"}"
k -k Tab; k ab; k -k Escape; sleep 3
check "one Escape in the Filename field refuses" '{"response":1}' "$(cat "$D/reply.json" 2> /dev/null)"
check "nothing left running" 0 "$(alive)"
APP=""

echo "screenshots in $D/shots"
echo "iced-headless: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
