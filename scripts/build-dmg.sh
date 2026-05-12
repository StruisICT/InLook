#!/usr/bin/env bash
# Build a macOS .app bundle and pack it into a .dmg.
# Expects a universal binary at dist/inlook (created upstream via `lipo`).
# Usage: scripts/build-dmg.sh <version>
set -euo pipefail

VERSION="${1:-}"
if [[ -z "$VERSION" ]]; then
    echo "usage: $0 <version>" >&2
    exit 2
fi
VERSION="${VERSION#v}"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DIST="$ROOT/dist"
APP="$DIST/InLook.app"
DMG="$DIST/InLook-${VERSION}-universal.dmg"

if [[ ! -x "$DIST/inlook" ]]; then
    echo "missing universal binary at $DIST/inlook" >&2
    exit 1
fi

rm -rf "$APP" "$DMG"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

# Binary
install -m 755 "$DIST/inlook" "$APP/Contents/MacOS/inlook"

# Info.plist with the version substituted in
sed "s/__VERSION__/${VERSION}/g" "$ROOT/assets/Info.plist" > "$APP/Contents/Info.plist"

# Icon: build a .icns from the committed hicolor PNGs. Falls back to sips
# downsampling the 1024px master if a particular committed size is missing.
ICNS_OUT="$APP/Contents/Resources/inlook.icns"
ICONSET="$DIST/inlook.iconset"
MASTER="$ROOT/assets/inlook.png"
rm -rf "$ICONSET"
mkdir -p "$ICONSET"
# (logical name, file size) — iconutil expects these canonical filenames.
# Each line is "<logical-size> <icns-suffix>"; the icns spec wants 1x and 2x
# variants from 16 up to 512.
copy_or_resize() {
    local pixels="$1" dest="$2"
    local src="$ROOT/assets/icons/inlook-${pixels}.png"
    if [[ -f "$src" ]]; then
        cp "$src" "$dest"
    else
        sips -z "$pixels" "$pixels" "$MASTER" --out "$dest" >/dev/null
    fi
}
copy_or_resize  16  "$ICONSET/icon_16x16.png"
copy_or_resize  32  "$ICONSET/icon_16x16@2x.png"
copy_or_resize  32  "$ICONSET/icon_32x32.png"
copy_or_resize  64  "$ICONSET/icon_32x32@2x.png"
copy_or_resize 128  "$ICONSET/icon_128x128.png"
copy_or_resize 256  "$ICONSET/icon_128x128@2x.png"
copy_or_resize 256  "$ICONSET/icon_256x256.png"
copy_or_resize 512  "$ICONSET/icon_256x256@2x.png"
copy_or_resize 512  "$ICONSET/icon_512x512.png"
copy_or_resize 1024 "$ICONSET/icon_512x512@2x.png"
iconutil -c icns "$ICONSET" -o "$ICNS_OUT"
/usr/libexec/PlistBuddy -c "Add :CFBundleIconFile string inlook.icns" \
    "$APP/Contents/Info.plist" 2>/dev/null || true

# Build the DMG. UDZO = compressed read-only.
hdiutil create -volname "InLook ${VERSION}" \
    -srcfolder "$APP" \
    -ov \
    -format UDZO \
    "$DMG"

echo "Built: $DMG"
ls -lh "$DMG"
