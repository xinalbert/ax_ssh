#!/bin/sh
set -eu

if [ "$#" -ne 1 ]; then
    printf 'Usage: sh packaging/macos/sign-app.sh <AxSSH.app>\n' >&2
    exit 1
fi

app_dir=$1
case "$app_dir" in
    /*) ;;
    *) app_dir="$(pwd)/$app_dir" ;;
esac

identifier=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$app_dir/Contents/Info.plist")
if [ "$identifier" != 'com.axssh.AxSSH' ]; then
    printf 'Refusing to sign a bundle with an unexpected AxSSH identifier\n' >&2
    exit 1
fi

# Sign the complete bundle only after all resources and architecture slices are
# final. The default ad-hoc requirement binds access to this exact program;
# never replace it with an identifier-only requirement to carry grants forward.
/usr/bin/codesign --force --sign - --identifier "$identifier" --timestamp=none "$app_dir"
/usr/bin/codesign --verify --deep --strict --verbose=2 "$app_dir"
