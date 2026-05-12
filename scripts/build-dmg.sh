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

# Icon: prefer a pre-built .icns, otherwise generate one from assets/inlook.png
# using the macOS-native sips + iconutil (always present on macos-latest runners).
ICNS_OUT="$APP/Contents/Resources/inlook.icns"
if [[ -f "$ROOT/assets/inlook.icns" ]]; then
    cp "$ROOT/assets/inlook.icns" "$ICNS_OUT"
elif [[ -f "$ROOT/assets/inlook.png" ]]; then
    ICONSET="$DIST/inlook.iconset"
    rm -rf "$ICONSET"
    mkdir -p "$ICONSET"
    SRC="$ROOT/assets/inlook.png"
    for size in 16 32 128 256 512; do
        sips -z $size $size "$SRC" --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
        sips -z $((size * 2)) $((size * 2)) "$SRC" --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
    done
    sips -z 1024 1024 "$SRC" --out "$ICONSET/icon_512x512@2x.png" >/dev/null
    iconutil -c icns "$ICONSET" -o "$ICNS_OUT"
fi
if [[ -f "$ICNS_OUT" ]]; then
    /usr/libexec/PlistBuddy -c "Add :CFBundleIconFile string inlook.icns" \
        "$APP/Contents/Info.plist" 2>/dev/null || true
fi

# Build the DMG. UDZO = compressed read-only.
hdiutil create -volname "InLook ${VERSION}" \
    -srcfolder "$APP" \
    -ov \
    -format UDZO \
    "$DMG"

echo "Built: $DMG"
ls -lh "$DMG"
