# Flatpak packaging

Source of truth for the Flathub submission of `com.struisict.InLook`.

## Files

| File | Purpose |
|---|---|
| `com.struisict.InLook.yaml` | Flatpak build manifest (used by `flatpak-builder`) |
| `com.struisict.InLook.metainfo.xml` | AppStream metadata (description, releases, screenshots) |
| `generated-sources.json` | Vendored Cargo sources for offline build (auto-generated) |

The first two are mirrored into the Flathub-owned repo
[`flathub/com.struisict.InLook`](https://github.com/flathub/com.struisict.InLook)
once Flathub accepts the initial submission. `generated-sources.json`
is committed alongside.

## Local build (Linux)

```sh
flatpak install -y flathub org.gnome.Platform//47 org.gnome.Sdk//47 \
  org.freedesktop.Sdk.Extension.rust-stable//24.08

flatpak-builder --user --install --force-clean build \
  packaging/flatpak/com.struisict.InLook.yaml

flatpak run com.struisict.InLook test/sample.eml
```

## Regenerating `generated-sources.json`

Whenever `Cargo.lock` changes, regenerate the vendored sources list.
The official `flatpak-cargo-generator.py` runs on any OS with Python
3.9+:

```sh
# One-time
pip install "aiohttp<4,>=3.9.5" "PyYAML<7,>=6" "tomlkit>=0.13.3,<1"

curl -fsSL -o /tmp/flatpak-cargo-generator.py \
  https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/master/cargo/flatpak-cargo-generator.py

# Every time Cargo.lock changes
python /tmp/flatpak-cargo-generator.py Cargo.lock \
  -o packaging/flatpak/generated-sources.json
```

The release CI does this automatically on every tag.

## Submitting to Flathub

Initial submission is a one-time PR to
[`flathub/flathub`](https://github.com/flathub/flathub) on the `new-pr`
branch. After acceptance, Flathub creates the dedicated app repo and
all future updates land there as ordinary PRs.

See <https://docs.flathub.org/docs/for-app-authors/submission> for the
current submission checklist.
