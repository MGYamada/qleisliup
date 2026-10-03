#!/bin/sh
# Exercise native manager commands without Rust, Lean, Python, or build tools.
# This checks local fixture operations; live distribution still needs production trust.
set -eu

project_root=$(CDPATH= cd -P "$(dirname "$0")/.." && pwd)
binary=${1:-"$project_root/target/release/qleisliup"}
binary_dir=$(CDPATH= cd -P "$(dirname "$binary")" && pwd)
binary="$binary_dir/$(basename "$binary")"
bootstrap=${2:-"$binary_dir/qleisliup-init"}
/bin/sh "$project_root/scripts/without_developer_tools.sh" \
    /bin/sh "$project_root/scripts/check_cli.sh" "$binary" "$bootstrap"
printf '%s\n' 'Cargo-free runtime smoke passed (local fixtures; live distribution remains unconfigured)'
