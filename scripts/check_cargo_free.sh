#!/bin/sh
# Exercise native manager commands with Rust developer tools trapped on PATH.
# This checks local fixture operations; live distribution still needs production trust.
set -eu

project_root=$(CDPATH= cd -P "$(dirname "$0")/.." && pwd)
binary=${1:-"$project_root/target/release/qleisliup"}
binary_dir=$(CDPATH= cd -P "$(dirname "$binary")" && pwd)
binary="$binary_dir/$(basename "$binary")"
bootstrap=${2:-"$binary_dir/qleisliup-init"}
guard_dir=$(mktemp -d "${TMPDIR:-/tmp}/qleisliup-cargo-free.XXXXXXXX")
trap 'rm -rf "$guard_dir"' 0
trap 'exit 1' HUP INT TERM

# Only the existing smoke harness's Unix utilities are available. No inherited
# PATH entry can provide a real cargo/rustc/rustdoc/rustup executable.
for utility in awk basename cat chmod dirname grep ln mkdir mktemp rm; do
    utility_path=$(command -v "$utility")
    case "$utility_path" in
        /*) ln -s "$utility_path" "$guard_dir/$utility" ;;
        *) printf '%s\n' "Expected an absolute executable for $utility" >&2; exit 1 ;;
    esac
done
for tool in cargo rustc rustdoc rustup; do
    cat >"$guard_dir/$tool" <<'TRAP'
#!/bin/sh
printf '%s\n' "$0" >>"$QLEISLIUP_DEVELOPER_TOOL_LOG"
printf '%s\n' 'Rust developer tools must not run during manager operations' >&2
exit 97
TRAP
    chmod 755 "$guard_dir/$tool"
done

status=0
PATH="$guard_dir" QLEISLIUP_DEVELOPER_TOOL_LOG="$guard_dir/invocations" \
    /bin/sh "$project_root/scripts/check_cli.sh" "$binary" "$bootstrap" || status=$?
if [ -e "$guard_dir/invocations" ]; then
    printf '%s\n' 'Cargo-free runtime check failed: a Rust developer tool was invoked' >&2
    cat "$guard_dir/invocations" >&2
    exit 1
fi
[ "$status" -eq 0 ] || exit "$status"
printf '%s\n' 'Cargo-free runtime smoke passed (local fixtures; live distribution remains unconfigured)'
