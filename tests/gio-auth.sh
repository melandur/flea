#!/usr/bin/env bash
# Proves trust warnings stop before one redacted password-only GIO exchange.
set -u
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1

dir="$FIXTURE_ROOT/flea-gio-auth-$$"
cleanup() { sandbox_remove "$dir"; }
trap cleanup EXIT HUP INT TERM
sandbox_make "$dir"
mkdir -p "$dir/bin"

fake_secret='not-a-real-password'
identity_status='unset'
certificate_status='unset'
cat > "$dir/bin/gio" <<'EOS'
#!/bin/sh
record_answer() {
    IFS= read -r answer || answer=
    printf '%s\n' "$answer" >> "$FAKE_GIO_RECEIVED"
}

case "$FAKE_GIO_FLOW" in
identity)
    printf 'Identity Verification Failed\n[1] Log In Anyway\n[2] Cancel Login\nChoice: '
    record_answer
    printf 'Password: '
    record_answer
    ;;
certificate)
    printf 'Certificate trust warning\nContinue anyway? y/n '
    record_answer
    printf 'Password: '
    record_answer
    ;;
password)
    printf 'Password: '
    record_answer
    printf 'Store password? [never/session/permanent] '
    record_answer
    ;;
no-prompt)
    exit 0
    ;;
*) exit 2 ;;
esac
EOS
chmod +x "$dir/bin/gio"

run_helper() {
    local flow=$1 received=$2 output=$3 store=${4:-}
    : > "$received"
    printf '%s\n' "$fake_secret" | FAKE_GIO_FLOW="$flow" FAKE_GIO_RECEIVED="$received" \
        FLEA_GIO_AUTH_TIMEOUT=5 PATH="$dir/bin:/usr/bin:/bin" \
        ./tools/flea-gio-auth 'sftp://user@example.test/' $store > "$output" 2>&1
}

for flow in identity certificate; do
    received="$dir/$flow.received"
    output="$dir/$flow.output"
    run_helper "$flow" "$received" "$output"
    rc=$?
    case "$flow" in
    identity) identity_status=$rc ;;
    certificate) certificate_status=$rc ;;
    esac
    [[ "$rc" -ne 0 ]] || { printf 'gio-auth: FAIL %s warning accepted\n' "$flow"; exit 1; }
    ! grep -Fq -- "$fake_secret" "$received" \
        || { printf 'gio-auth: FAIL %s warning received password\n' "$flow"; exit 1; }
    [[ ! -s "$output" ]] \
        || { printf 'gio-auth: FAIL %s warning produced output\n' "$flow"; exit 1; }
done

received="$dir/no-prompt.received"
output="$dir/no-prompt.output"
run_helper no-prompt "$received" "$output"
rc=$?
no_prompt_status=$rc
[[ "$rc" -ne 0 ]] || { printf 'gio-auth: FAIL no-prompt child accepted\n'; exit 1; }
[[ ! -s "$received" ]] || { printf 'gio-auth: FAIL no-prompt child received input\n'; exit 1; }
[[ ! -s "$output" ]] || { printf 'gio-auth: FAIL no-prompt helper produced output\n'; exit 1; }

received="$dir/password.received"
output="$dir/password.output"
run_helper password "$received" "$output"
rc=$?
password_status=$rc
[[ "$rc" -eq 0 ]] || { printf 'gio-auth: FAIL password-only helper=%s\n' "$rc"; exit 1; }
[[ "$(grep -Fxc -- "$fake_secret" "$received")" -eq 1 ]] \
    || { printf 'gio-auth: FAIL password delivery was not exactly once\n'; exit 1; }
[[ "$(sed -n '2p' "$received")" == never ]] \
    || { printf 'gio-auth: FAIL storage response was not never\n'; exit 1; }
[[ ! -s "$output" ]] \
    || { printf 'gio-auth: FAIL password-only helper produced output\n'; exit 1; }

# Settings > Places > Remember passwords is the only thing that answers that prompt differently,
# and it reaches this helper as an argument: never by default, permanent when the switch is on, and
# nothing else at all, because the answer becomes a keyring write.
received="$dir/permanent.received"
output="$dir/permanent.output"
run_helper password "$received" "$output" permanent
rc=$?
[[ "$rc" -eq 0 ]] || { printf 'gio-auth: FAIL permanent helper=%s\n' "$rc"; exit 1; }
[[ "$(sed -n '2p' "$received")" == permanent ]] \
    || { printf 'gio-auth: FAIL storage response was not permanent\n'; exit 1; }
[[ ! -s "$output" ]] || { printf 'gio-auth: FAIL permanent helper produced output\n'; exit 1; }

received="$dir/session.received"
run_helper password "$received" "$dir/session.output" session
[[ "$(sed -n '2p' "$received")" == session ]] \
    || { printf 'gio-auth: FAIL storage response was not session\n'; exit 1; }

received="$dir/bogus.received"
run_helper password "$received" "$dir/bogus.output" forever
rc=$?
[[ "$rc" -eq 2 ]] || { printf 'gio-auth: FAIL an unknown storage choice was accepted, helper=%s\n' "$rc"; exit 1; }
[[ ! -s "$received" ]] || { printf 'gio-auth: FAIL a refused storage choice still spawned gio\n'; exit 1; }

printf 'gio-auth: identity=%s certificate=%s no-prompt=%s password=%s once=ok storage=never,permanent,session redaction=ok\n' \
    "$identity_status" "$certificate_status" "$no_prompt_status" "$password_status"
