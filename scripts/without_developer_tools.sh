#!/bin/sh
# Run an explicitly selected native command with developer tools trapped on PATH.
# This is a dependency check, not a filesystem, process, or network sandbox.
set -eu

if [ "$#" -eq 0 ]; then
    printf '%s\n' 'Usage: without_developer_tools.sh /absolute/command [argument ...]' >&2
    exit 2
fi
case "$1" in
    /*) ;;
    *) printf '%s\n' 'The checked command must be an absolute path' >&2; exit 2 ;;
esac

guard_dir=$(mktemp -d "${TMPDIR:-/tmp}/qleisliup-runtime-guard.XXXXXXXX")
trap 'rm -rf "$guard_dir"' 0
trap 'exit 1' HUP INT TERM

# Allow only the Unix utilities used by the native smoke harness. Other names
# cannot resolve through the caller's PATH. Absolute paths remain executable.
for utility in awk basename cat chmod dirname grep ln mkdir mktemp rm; do
    utility_path=$(command -v "$utility")
    case "$utility_path" in
        /*) ln -s "$utility_path" "$guard_dir/$utility" ;;
        *) printf '%s\n' "Expected an absolute executable for $utility" >&2; exit 1 ;;
    esac
done
for tool in cargo rustc rustdoc rustup lean lake elan leanc \
    python python2 python3 pip pip2 pip3 uv uvx \
    cc c++ gcc g++ clang clang++ cmake make ninja; do
    cat >"$guard_dir/$tool" <<'TRAP'
#!/bin/sh
printf '%s\n' "$0" >>"$QLEISLIUP_DEVELOPER_TOOL_LOG"
printf '%s\n' 'Developer tools must not run during native toolchain operations' >&2
exit 97
TRAP
    chmod 755 "$guard_dir/$tool"
done

status=0
PATH="$guard_dir" QLEISLIUP_DEVELOPER_TOOL_LOG="$guard_dir/invocations" \
    "$@" || status=$?
if [ -e "$guard_dir/invocations" ]; then
    printf '%s\n' 'Native runtime check failed: a developer tool was invoked' >&2
    cat "$guard_dir/invocations" >&2
    exit 1
fi
exit "$status"
