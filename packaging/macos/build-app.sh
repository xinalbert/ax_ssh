#!/bin/sh
set -eu

root_dir=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
app_dir=${1:-"$root_dir/dist/AxSSH.app"}

cargo build --release --locked --manifest-path "$root_dir/Cargo.toml"
mkdir -p "$app_dir/Contents/MacOS" "$app_dir/Contents/Resources"
cp "$root_dir/target/release/ax_ssh" "$app_dir/Contents/MacOS/AxSSH"
cp "$root_dir/packaging/macos/Info.plist" "$app_dir/Contents/Info.plist"
cp "$root_dir/assets/ion/terminal_icon_all_formats/terminal_icon.icns" "$app_dir/Contents/Resources/AxSSH.icns"
cp "$root_dir/LICENSE" "$app_dir/Contents/Resources/LICENSE"
cp "$root_dir/THIRD_PARTY_NOTICES.md" "$app_dir/Contents/Resources/THIRD_PARTY_NOTICES.md"
chmod +x "$app_dir/Contents/MacOS/AxSSH"

printf 'Created %s\n' "$app_dir"
