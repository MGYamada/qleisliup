#!/bin/sh
# Smoke-check read-only CLI, local registration, and compiler proxy dispatch.
set -eu

project_root=$(CDPATH= cd -P "$(dirname "$0")/.." && pwd)
binary=${1:-"$project_root/target/debug/qleisliup"}
binary_dir=$(CDPATH= cd -P "$(dirname "$binary")" && pwd)
binary="$binary_dir/$(basename "$binary")"
bootstrap=${2:-"$binary_dir/qleisliup-init"}
# Cargo normalizes dependency versions onto their own lines in packaged manifests.
version=$(awk '
    /^\[/ { package = ($0 == "[package]") }
    package && /^version = "/ { split($0, parts, "\""); print parts[2]; exit }
' "$project_root/Cargo.toml")
smoke_dir=$(mktemp -d "${TMPDIR:-/tmp}/qleisliup-smoke.XXXXXXXX")
trap 'rm -rf "$smoke_dir"' 0
trap 'exit 1' HUP INT TERM
export QLEISLIUP_HOME="$smoke_dir/manager-home"
unset QLEISLIUP_TOOLCHAIN

fail() {
    printf '%s\n' "CLI smoke check failed: $*" >&2
    exit 1
}

check_rejected() {
    status=0
    "$binary" "$@" >"$smoke_dir/out" 2>"$smoke_dir/err" || status=$?
    [ "$status" -eq 2 ] || fail "expected status 2 for $*; got $status"
    [ ! -s "$smoke_dir/out" ] || fail "unexpected stdout for $*"
    [ -s "$smoke_dir/err" ] || fail "missing diagnostic for $*"
}

[ -n "$version" ] || fail "missing Cargo package version"
for flag in --version -V; do
    "$binary" "$flag" >"$smoke_dir/out" 2>"$smoke_dir/err"
    [ "$(cat "$smoke_dir/out")" = "qleisliup $version" ] || fail "version output"
    [ ! -s "$smoke_dir/err" ] || fail "unexpected version diagnostic"
done
for flag in --help -h; do
    "$binary" "$flag" >"$smoke_dir/out" 2>"$smoke_dir/err"
    grep -F 'Stage 4, bootstrap and manager self-update' "$smoke_dir/out" >/dev/null || fail "help status"
    grep -F 'Production distribution URLs and trusted root are not configured' "$smoke_dir/out" >/dev/null || fail "distribution status"
    grep -F 'Self update requires a bootstrap-owned manager' "$smoke_dir/out" >/dev/null || fail "update ownership status"
    [ ! -s "$smoke_dir/err" ] || fail "unexpected help diagnostic"
done
"$binary" >"$smoke_dir/out" 2>"$smoke_dir/err"
grep -F 'Stage 4, bootstrap and manager self-update' "$smoke_dir/out" >/dev/null || fail "default help"
[ ! -s "$smoke_dir/err" ] || fail "unexpected default diagnostic"

"$binary" list >"$smoke_dir/out" 2>"$smoke_dir/err"
grep -F 'No installed toolchains' "$smoke_dir/out" >/dev/null || fail "empty list"
[ ! -s "$smoke_dir/err" ] || fail "unexpected list diagnostic"

for command in install uninstall self; do
    check_rejected "$command"
done
for selector in 0.4.0 stable; do
    status=0
    "$binary" install "$selector" >"$smoke_dir/out" 2>"$smoke_dir/err" || status=$?
    [ "$status" -eq 1 ] || fail "unconfigured install status"
    [ ! -s "$smoke_dir/out" ] || fail "unconfigured install reported success"
    grep -F 'production distribution is not configured' "$smoke_dir/err" >/dev/null || fail "unconfigured install diagnostic"
done
check_rejected install dev
check_rejected uninstall stable
check_rejected sync extra
check_rejected toolchain
check_rejected toolchain link
check_rejected toolchain link stable /unused/toolchain
check_rejected self update extra
check_rejected unknown-command
check_rejected --version extra
check_rejected --help extra
check_rejected --format=json
check_rejected pin stable
check_rejected default dev
check_rejected which unknown-tool
[ ! -e "$QLEISLIUP_HOME" ] || fail "read-only CLI created manager state"

status=0
"$binary" self update >"$smoke_dir/out" 2>"$smoke_dir/err" || status=$?
[ "$status" -eq 1 ] || fail "external self update status"
[ ! -s "$smoke_dir/out" ] || fail "external self update reported success"
grep -F 'original installation method' "$smoke_dir/err" >/dev/null || fail "external self update diagnostic"

for flag in --version -V; do
    "$bootstrap" "$flag" >"$smoke_dir/out" 2>"$smoke_dir/err"
    [ "$(cat "$smoke_dir/out")" = "qleisliup-init $version" ] || fail "bootstrap version"
    [ ! -s "$smoke_dir/err" ] || fail "bootstrap version diagnostic"
done
for flag in --help -h; do
    "$bootstrap" "$flag" >"$smoke_dir/out" 2>"$smoke_dir/err"
    grep -F 'Production distribution URLs and trusted root are not configured' "$smoke_dir/out" >/dev/null || fail "bootstrap help"
    [ ! -s "$smoke_dir/err" ] || fail "bootstrap help diagnostic"
done
status=0
"$bootstrap" >"$smoke_dir/out" 2>"$smoke_dir/err" || status=$?
[ "$status" -eq 1 ] || fail "unconfigured bootstrap status"
[ ! -s "$smoke_dir/out" ] || fail "unconfigured bootstrap reported success"
grep -F 'production distribution is not configured' "$smoke_dir/err" >/dev/null || fail "unconfigured bootstrap diagnostic"
status=0
"$bootstrap" install >"$smoke_dir/out" 2>"$smoke_dir/err" || status=$?
[ "$status" -eq 2 ] || fail "bootstrap syntax status"
[ ! -e "$QLEISLIUP_HOME" ] || fail "rejected lifecycle requests created manager state"

mkdir -p "$smoke_dir/local/bin" "$smoke_dir/proxies"
cat >"$smoke_dir/local/bin/qleisli" <<'TOOL'
#!/bin/sh
[ "$QLEISLIUP_TOOLCHAIN" = "dev" ] || exit 91
printf '%s\n' "fixture:$1"
TOOL
chmod 755 "$smoke_dir/local/bin/qleisli"
"$binary" toolchain link dev "$smoke_dir/local" >"$smoke_dir/out" 2>"$smoke_dir/err"
grep -F 'local, unauthenticated' "$smoke_dir/out" >/dev/null || fail "link authentication status"
[ ! -s "$smoke_dir/err" ] || fail "unexpected link diagnostic"
for tool in qli qleisli; do
    ln -s "$binary" "$smoke_dir/proxies/$tool"
    "$smoke_dir/proxies/$tool" +dev --version >"$smoke_dir/out" 2>"$smoke_dir/err"
    [ "$(cat "$smoke_dir/out")" = 'fixture:--version' ] || fail "$tool forwarding"
    [ ! -s "$smoke_dir/err" ] || fail "unexpected $tool diagnostic"
done
"$binary" toolchain unlink dev >"$smoke_dir/out" 2>"$smoke_dir/err"
[ ! -s "$smoke_dir/err" ] || fail "unexpected unlink diagnostic"
[ -f "$smoke_dir/local/bin/qleisli" ] || fail "unlink removed the build"

printf '%s\n' "CLI smoke check passed for qleisliup $version"
