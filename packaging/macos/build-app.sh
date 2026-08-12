#!/bin/sh
set -eu

root_dir=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
app_dir=${1:-"$root_dir/dist/AxSSH.app"}
build_revision=${AXSSH_BUILD_REVISION:-unknown}
if [ "$build_revision" = unknown ] && command -v git >/dev/null 2>&1; then
    build_revision=$(git -C "$root_dir" rev-parse --short=12 HEAD 2>/dev/null || printf 'unknown')
fi

AXSSH_BUILD_REVISION="$build_revision" cargo build --release --locked --manifest-path "$root_dir/Cargo.toml"
mkdir -p "$app_dir/Contents/MacOS" "$app_dir/Contents/Resources"
cp "$root_dir/target/release/ax_ssh" "$app_dir/Contents/MacOS/AxSSH"
cp "$root_dir/packaging/macos/Info.plist" "$app_dir/Contents/Info.plist"
cp "$root_dir/assets/ion/terminal_icon_all_formats/terminal_icon.icns" "$app_dir/Contents/Resources/AxSSH.icns"
mkdir -p "$app_dir/Contents/Resources/assets/fonts"
cp -R "$root_dir/assets/fonts/." "$app_dir/Contents/Resources/assets/fonts/"
cp "$root_dir/LICENSE" "$app_dir/Contents/Resources/LICENSE"
cp "$root_dir/THIRD_PARTY_NOTICES.md" "$app_dir/Contents/Resources/THIRD_PARTY_NOTICES.md"
chmod +x "$app_dir/Contents/MacOS/AxSSH"

printf 'Created %s\n' "$app_dir"
