#!/bin/sh
# Builds an unsigned disk image holding Tidewater.app and an Applications link.
# Usage: build-dmg.sh VERSION BINARY OUT.dmg
# The binary is the only thing packaged: no game data ever goes in.
set -eu
version=$1
binary=$2
out=$3
here=$(cd "$(dirname "$0")" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

app="$work/dmg/Tidewater.app"
mkdir -p "$app/Contents/MacOS"
cp "$binary" "$app/Contents/MacOS/tidewater"
chmod 755 "$app/Contents/MacOS/tidewater"
sed "s/@VERSION@/$version/g" "$here/Info.plist" > "$app/Contents/Info.plist"
ln -s /Applications "$work/dmg/Applications"

rm -f "$out"
hdiutil create -quiet -volname "Tidewater" -srcfolder "$work/dmg" -ov -format UDZO "$out"
echo "wrote $out"
