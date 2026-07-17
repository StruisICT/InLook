# InLook

**Fast, safe viewer for `.eml` and Outlook `.msg` email files.**
Free Software from **Struis ICT**.

InLook opens an email file (`.eml`, Outlook `.msg`, or `.oft` template) in a
clean native window and renders its headers, body, and attachment list —
without phoning home, loading remote trackers, or running any scripts the
email tries to sneak in.

- **Tiny & native** — a single Rust binary using [`tao`](https://crates.io/crates/tao)
  for the window and [`wry`](https://crates.io/crates/wry) (WebView2 on Windows,
  WebKitGTK on Linux, WKWebView on macOS) to render the email body.
- **Safe by default** — HTML bodies are wrapped in a fully sandboxed `<iframe>`
  with a strict Content-Security-Policy. No remote images, no tracking pixels,
  no scripts, no network. Embedded (`cid:`) images render inline from the
  message itself — never from the network.
- **Attachments** — click to save any attachment (always via Save As, never
  auto-run); attached emails open in a new InLook window.
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
inlook <file>         Open an .eml / .msg / .oft email file
inlook                Open a file picker
inlook register       Associate .eml/.msg/.oft with InLook   (Windows, admin)
inlook unregister     Remove the file associations           (Windows, admin)
inlook --version
inlook --help
```

On Windows, `inlook register` (run from an **elevated** terminal) registers
InLook as a handler for `.eml`, `.msg`, and `.oft` files and opens Windows
Settings on InLook's Default Apps page to finish with one click.

> **`.msg` note:** InLook shows the HTML or plain-text body stored in the
> message. Messages whose body exists *only* as compressed RTF (rare — mostly
> very old Outlook versions) render their headers and attachment list with an
> empty body.

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

## Versioning

InLook follows [Semantic Versioning 2.0.0](https://semver.org/)
(`MAJOR.MINOR.PATCH`). For an app, the "public API" is the user-facing
contract: the command-line flags/subcommands and their exit codes, the `.eml`
file association, and the published package identifiers. MAJOR = a
backwards-incompatible change to that contract, MINOR = a backwards-compatible
new capability, PATCH = a backwards-compatible fix. While at `0.x` (initial
development) the contract may still change. Releases are automated from
[Conventional Commits](https://www.conventionalcommits.org/) via release-please;
see [`AGENTS.md` §5.1](AGENTS.md) for the full policy.

## Code signing policy

Free code signing on Windows provided by [SignPath.io](https://about.signpath.io/),
certificate by [SignPath Foundation](https://signpath.org/).

- **Committers and reviewers:** [Struis112](https://github.com/Struis112)
- **Approvers:** [Struis112](https://github.com/Struis112)

Windows release binaries (`inlook.exe`, the `.msi`) are built from source by
GitHub Actions ([`release.yml`](.github/workflows/release.yml)) and signed per
release after manual approval.

### Privacy policy

InLook is **offline by default**: it has no telemetry and phones nothing home.
Email content stays on your machine, and remote content inside emails is never
loaded (blocked by CSP + iframe sandbox).

The **only** time InLook makes a network connection is if you explicitly opt in
to the update check (Windows only). On first run it asks once whether to check
for updates; if you say yes, it occasionally contacts `github.com` over HTTPS —
using Windows' own secure connection, with no third-party HTTP or TLS code — to
read the latest release tag and, at most once per version, tell you a newer
version exists. It never downloads or installs anything, and sends no
information about you or your email. If you say no (or never opt in), InLook
makes no network connection at all. You can change your choice anytime via the
registry value `HKCU\Software\StruisICT\InLook\UpdateCheckEnabled` (`0`/`1`).

## License

Dual-licensed under **MIT OR Apache-2.0**. See [`LICENSE`](LICENSE).

---

© Struis ICT — <https://struisict.com> · [Buy me a coffee ☕](https://buymeacoffee.com/struis112)
