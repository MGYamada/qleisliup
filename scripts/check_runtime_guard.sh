#!/bin/sh
# Check that swallowed developer-tool failures still reject a native scenario.
set -eu

project_root=$(CDPATH= cd -P "$(dirname "$0")/.." && pwd)
runner="$project_root/scripts/without_developer_tools.sh"
test_dir=$(mktemp -d "${TMPDIR:-/tmp}/qleisliup-guard-tests.XXXXXXXX")
trap 'rm -rf "$test_dir"' 0
trap 'exit 1' HUP INT TERM

fail() {
    printf '%s\n' "Runtime guard check failed: $*" >&2
    exit 1
}

status=0
/bin/sh "$runner" >"$test_dir/out" 2>"$test_dir/err" || status=$?
[ "$status" -eq 2 ] || fail 'missing command must fail with status 2'
status=0
/bin/sh "$runner" relative-command >"$test_dir/out" 2>"$test_dir/err" || status=$?
[ "$status" -eq 2 ] || fail 'relative command must fail with status 2'

/bin/sh "$runner" /bin/sh -c 'printf "%s\n" "$1" | cat' fixture 'argument with spaces' \
    >"$test_dir/out" 2>"$test_dir/err"
[ "$(cat "$test_dir/out")" = 'argument with spaces' ] || fail 'argument/output transport'
[ ! -s "$test_dir/err" ] || fail 'unexpected success diagnostic'
status=0
/bin/sh "$runner" /bin/sh -c 'exit 42' >"$test_dir/out" 2>"$test_dir/err" || status=$?
[ "$status" -eq 42 ] || fail 'command exit status was not preserved'

# This caller-PATH command must never be reached, even if a trap is missing.
mkdir "$test_dir/caller-bin"
cat >"$test_dir/caller-bin/unlisted-developer-tool" <<'TOOL'
#!/bin/sh
printf '%s\n' reached >"$QLEISLIUP_GUARD_TEST_MARKER"
TOOL
chmod 755 "$test_dir/caller-bin/unlisted-developer-tool"
PATH="$test_dir/caller-bin:$PATH" QLEISLIUP_GUARD_TEST_MARKER="$test_dir/reached" \
    /bin/sh "$runner" /bin/sh -c '! command -v unlisted-developer-tool' \
    >"$test_dir/out" 2>"$test_dir/err"
[ ! -e "$test_dir/reached" ] || fail 'inherited PATH leaked into the scenario'

# Cover each supported trap name, including failures ignored by the scenario.
for tool in cargo rustc rustdoc rustup lean lake elan leanc \
    python python2 python3 pip pip2 pip3 uv uvx \
    cc c++ gcc g++ clang clang++ cmake make ninja; do
    ln -s unlisted-developer-tool "$test_dir/caller-bin/$tool"
    status=0
    PATH="$test_dir/caller-bin:$PATH" QLEISLIUP_GUARD_TEST_MARKER="$test_dir/reached" \
        /bin/sh "$runner" /bin/sh -c '"$1" --version >/dev/null 2>&1 || :' fixture "$tool" \
        >"$test_dir/out" 2>"$test_dir/err" || status=$?
    [ "$status" -eq 1 ] || fail "swallowed $tool invocation was accepted"
    grep -F 'Native runtime check failed: a developer tool was invoked' "$test_dir/err" \
        >/dev/null || fail "missing $tool diagnostic"
    grep -F "/$tool" "$test_dir/err" >/dev/null || fail "missing $tool invocation log"
    [ ! -e "$test_dir/reached" ] || fail "caller-PATH $tool was executed"
done

printf '%s\n' 'Runtime guard checks passed (Rust, Lean, Python, and native build tools)'
