# InLook

**Fast, safe `.eml` email viewer.** Free Software from **Struis ICT**.

InLook opens a `.eml` file in a clean native window and renders its headers,
body, and attachment list — without phoning home, loading remote trackers, or
running any scripts the email tries to sneak in.

- **Tiny & native** — a single Rust binary using [`tao`](https://crates.io/crates/tao)
  for the window and [`wry`](https://crates.io/crates/wry) (WebView2 on Windows,
  WebKitGTK on Linux, WKWebView on macOS) to render the email body.
- **Safe by default** — HTML bodies are wrapped in a fully sandboxed `<iframe>`
  with a strict Content-Security-Policy. No remote images, no tracking pixels,
  no scripts, no network. Inline `data:` images only.
- **Cross-platform** — Windows (MSI + exe), Linux (`.deb` + AppImage + Flatpak),
  macOS (universal `.dmg`), Homebrew cask.
- **System theme aware** — follows the OS light/dark setting automatically.

## Install

Pre-built binaries are attached to each
[GitHub Release](https://github.com/StruisICT/InLook/releases).

| Platform | Package |
|---|---|
| Windows | `inlook-*.msi` (installer) or standalone `inlook.exe` |
| Linux (Debian/Ubuntu) | `inlook_*.deb` |
| Linux (any) | `InLook-*.AppImage` |
| Linux (Flatpak) | `com.struisict.InLook` (Flathub, once accepted) |
| macOS | `inlook-*.dmg` (universal: Apple Silicon + Intel) |
| macOS (Homebrew) | `brew install --cask inlook` (from the cask source) |

> **Windows note:** the body renderer needs the **Microsoft Edge WebView2
> Runtime**, which ships with Windows 10/11 by default. If missing, InLook shows
> a clear error telling you to install it.

## Usage

```
inlook <file.eml>     Open an EML file in the viewer window
inlook                Open a file picker
inlook register       Associate .eml with InLook        (Windows, admin)
inlook unregister     Remove the .eml association        (Windows, admin)
inlook --version
inlook --help
```

On Windows, `inlook register` (run from an **elevated** terminal) makes InLook
the default handler for `.eml` files so double-clicking one opens it here.

## Build from source

Requires a recent stable Rust toolchain (built with 1.95; MSRV is **1.74**).

```sh
cargo build --release
cargo run --release -- test/sample.eml
```

On Linux you also need the WebKitGTK/GTK dev packages:

```sh
sudo apt-get install -y libwebkit2gtk-4.1-dev libgtk-3-dev \
  libayatana-appindicator3-dev librsvg2-dev
```

See [`AGENTS.md`](AGENTS.md) for the full build, test, packaging, and release
workflow.

## License

Dual-licensed under **MIT OR Apache-2.0**. See [`LICENSE`](LICENSE).

---

© Struis ICT — <https://struisict.com> · [Buy me a coffee ☕](https://buymeacoffee.com/struis112)
