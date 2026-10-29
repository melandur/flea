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
[ -x "$BIN" ] || { echo "iced-headless.sh: $BIN is missing, run cargo build" >&2; exit 1; }
[ -x "$ICED" ] || { echo "iced-headless.sh: $ICED is missing, run cargo build in crates/flea-iced" >&2; exit 1; }
for tool in sway swaymsg wtype grim jq wl-paste; do
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
        XDG_STATE_HOME="$D/state" XDG_CACHE_HOME="$D/cache" XDG_DATA_HOME="$D/data" \
        FLEA_BIN="$BIN" FLEA_ICED_TRACE=1 FLEA_PICKER_REPLY="$D/reply.json" \
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
# Duplicate: five Downs. Show hidden files is the last row, and Down holds at the end.
top; k -k Down -k Down; k m; k -k Down -k Down -k Down -k Down -k Down; k -k Return; sleep 1
check "the menu's Duplicate copies the cursor row" yes "$(exists "$T/note-02 copy.txt")"
k m; for _ in 1 2 3 4 5 6 7 8 9 10; do k -k Down; done; k -k Return; sleep 1
check "the menu's last row shows hidden files" true "$(jq -r .hidden "$D/state/flea/ui.json")"
k m; for _ in 1 2 3 4 5 6 7 8 9 10; do k -k Down; done; k -k Return; sleep 1
check "and the same row, relabelled, hides them" false "$(jq -r .hidden "$D/state/flea/ui.json")"
top; ctrl b; sleep 1
check "Ctrl+B copies the cursor row's path" "$T/alpha" "$(WAYLAND_DISPLAY=$SOCK timeout 3 wl-paste -n 2> /dev/null)"
k -k Tab; k -k Down -k Down; shot rail; k -k Tab
n=$(mark); backend=$(pgrep -P "$APP" -x flea)
comp "[app_id=\"$WANT\"] kill" > /dev/null; sleep 3
check "closing the window sends quit" 1 "$(since "$n" | grep -c '"c":"quit"')"
check "and the window and its backend both exit" "0 0" "$(alive) $(kill -0 "$backend" 2> /dev/null && echo 1 || echo 0)"

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
