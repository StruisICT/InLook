#!/usr/bin/env bash
# Build an AppImage from the release binary. Runs on ubuntu-latest in CI.
# Usage: scripts/build-appimage.sh <version>      (version like v0.3.0 or 0.3.0)
set -euo pipefail

VERSION="${1:-}"
if [[ -z "$VERSION" ]]; then
    echo "usage: $0 <version>" >&2
    exit 2
fi
# Strip leading 'v' if present.
VERSION="${VERSION#v}"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$ROOT/dist/AppDir"
OUT="$ROOT/dist"

mkdir -p "$OUT"
rm -rf "$WORK"
mkdir -p "$WORK/usr/bin" "$WORK/usr/share/applications" "$WORK/usr/share/icons/hicolor/256x256/apps"

# Binary
install -Dm755 "$ROOT/target/release/inlook" "$WORK/usr/bin/inlook"

# Desktop file — also placed at the AppDir root (AppImage convention)
install -Dm644 "$ROOT/assets/inlook.desktop" "$WORK/usr/share/applications/inlook.desktop"
cp "$ROOT/assets/inlook.desktop" "$WORK/inlook.desktop"

# Icon — fall back to a generated 256x256 PNG so AppImage tools don't complain
ICON_PNG="$WORK/usr/share/icons/hicolor/256x256/apps/inlook.png"
if [[ -f "$ROOT/assets/inlook.png" ]]; then
    cp "$ROOT/assets/inlook.png" "$ICON_PNG"
else
    # Minimal 256x256 PNG placeholder generated with ImageMagick (preinstalled
    # on ubuntu-latest). Keeps the AppImage self-contained without a real icon.
    convert -size 256x256 xc:'#2c5282' \
        -gravity center -pointsize 96 -fill white -annotate 0 'IL' \
        "$ICON_PNG"
fi
cp "$ICON_PNG" "$WORK/inlook.png"

# AppRun launcher — the AppImage entrypoint that execs the binary
cat > "$WORK/AppRun" <<'EOF'
#!/usr/bin/env bash
HERE="$(dirname "$(readlink -f "${0}")")"
exec "${HERE}/usr/bin/inlook" "$@"
EOF
chmod +x "$WORK/AppRun"

# Fetch appimagetool if not already cached on the runner
TOOL="${RUNNER_TEMP:-/tmp}/appimagetool"
if [[ ! -x "$TOOL" ]]; then
    curl -fsSL -o "$TOOL" \
        https://github.com/AppImage/AppImageKit/releases/download/continuous/appimagetool-x86_64.AppImage
    chmod +x "$TOOL"
fi

OUT_FILE="$OUT/InLook-${VERSION}-x86_64.AppImage"
ARCH=x86_64 "$TOOL" --no-appstream "$WORK" "$OUT_FILE"

echo "Built: $OUT_FILE"
ls -lh "$OUT_FILE"
