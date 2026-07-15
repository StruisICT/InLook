# AGENTS.md — working instructions for InLook

> **Read this first.** This is the standing context for InLook so you (or any AI
> tool / new machine) can resume work without re-learning the basics. Keep it up
> to date: when the architecture, commands, conventions, or roadmap change,
> update this file in the same change.

## 1. What this project is

**InLook** — a small, fast, *safe* `.eml` (RFC 822 email) viewer. Free Software
from **Struis ICT**. It opens one email file and renders headers + body +
attachment list in a native window. It is intentionally a *viewer*, not a mail
client: no accounts, no network, no sending.

- **Language:** Rust (edition 2021, MSRV **1.74**, developed on 1.95).
- **Crate name / binary:** `inlook`.
- **Canonical repo:** <https://github.com/StruisICT/InLook> (`origin`).
- **License:** `MIT OR Apache-2.0`.
- **Current version:** see `.release-please-manifest.json` (source of truth).

## 2. Architecture (the whole thing is ~630 lines)

| File | Responsibility |
|---|---|
| `src/lib.rs` | Library crate root. Exposes the pure, GUI-free core (`pub mod render`) so it can be unit-, snapshot- and fuzz-tested without a window. Keep it free of I/O and platform glue. |
| `src/main.rs` | Binary (thin shell over the lib). CLI arg parsing (`--version`, `--help`, `register`, `unregister`, `<file>`, or no-arg file picker). Reads the file (size-capped), builds the `tao` window + `wry` WebView, runs the event loop. Windows console-attach shim for CLI output. |
| `src/render.rs` | Pure function `render_eml_to_html(bytes, path) -> String`. Parses with `mail-parser`, formats headers (escaped), renders the body, lists attachments, and emits one self-contained HTML page. **All the security lives here.** Has the unit tests. |
| `src/registry.rs` | Windows-only, binary-only. `register()`/`unregister()` write the `.eml` file association into `HKLM\Software\Classes` (ProgID `StruisICT.InLook`) **plus Default Programs registration** (`RegisteredApplications` + `Capabilities`), notify the shell, then deep-link into Settings (`ms-settings:defaultapps?registeredAppMachine=InLook`) so the user finishes with one click — Windows never lets an app set the UserChoice default itself. Requires elevation. |
| `tests/snapshots.rs` | Golden-file snapshot tests: renders every `tests/fixtures/*.eml` and compares against `tests/snapshots/*.html`. Regenerate with `INLOOK_UPDATE_SNAPSHOTS=1 cargo test --test snapshots`, then review the diff. |
| `fuzz/` | cargo-fuzz target `render_eml` (detached workspace, nightly-only). CI smoke-fuzzes it via `.github/workflows/fuzz.yml` (render-path PRs, weekly, manual). |

**Data flow:** `file path → read_eml() (≤50 MiB) → render_eml_to_html() → wry WebView`.

### Key constants (don't loosen without a security reason)
- `MAX_FILE_BYTES = 50 MiB` (`main.rs`) — refuse larger files (DoS guard).
- `MAX_BODY_BYTES = 5 MiB` (`render.rs`) — truncate any single body part.

### Security model — preserve these invariants
- Every header value is HTML-escaped (`html_escape::encode_text`).
- HTML email bodies are placed inside an **empty-sandbox** `<iframe sandbox="">`
  via `srcdoc`, *and* the inner document carries a strict CSP
  (`default-src 'none'; img-src data:; ...`). Two independent layers.
- The outer page also has a strict CSP. No remote anything; inline `data:`
  images only. No scripts ever run from email content.
- `#![deny(unsafe_code)]` is on. The only `unsafe` is explicitly
  `#[allow(unsafe_code)]`-annotated Win32 calls (console attach, shell notify)
  with a `// Reason:` comment. Keep that pattern for any new FFI.

## 3. Everyday commands

```sh
cargo build --release                 # build the binary
cargo run --release -- test/sample.eml  # run against the sample email
cargo test --release                  # unit tests (render.rs) + snapshot tests (tests/)
INLOOK_UPDATE_SNAPSHOTS=1 cargo test --test snapshots  # regenerate golden snapshots
cargo +nightly fuzz run render_eml -- -max_total_time=60  # fuzz (Linux/macOS, needs cargo-fuzz)
cargo fmt --all                       # format
cargo fmt --all -- --check            # CI format gate
cargo clippy --all-targets --release -- -D warnings  # CI lint gate (warnings = errors)
```

**Before pushing, run the same gates CI runs:** `fmt --check`, `clippy -D warnings`,
`test --release`, `build --release`. CI (`.github/workflows/checks.yml`) runs them
on Windows, Linux, and macOS, plus a `cargo audit` security check.

Linux dev/CI system deps (WebKitGTK stack for `wry`):
```sh
sudo apt-get install -y libwebkit2gtk-4.1-dev libgtk-3-dev \
  libayatana-appindicator3-dev librsvg2-dev
```

## 4. Conventions

- **Commits:** Conventional Commits (`feat:`, `fix:`, `chore:`, `ci:`, `docs:`,
  …, with optional scope like `feat(packaging):`). This drives release-please
  versioning and the changelog — so commit messages matter. See
  [§5.1 Versioning (SemVer)](#51-versioning-semver) for the commit→version map.
- **Branches:** topic branches like `feat/...`, `fix/...`, `chore/...`; PR into
  `main`. `main` is the release line.
- **Style:** rustfmt defaults; clippy must be clean with `-D warnings`.
- **Tests:** unit tests live next to the code (`#[cfg(test)] mod tests` in
  `render.rs`); rendered-output changes are covered by the golden snapshots in
  `tests/`. Any new body/header/attachment handling should add a test,
  especially escaping/sandboxing assertions, and new render behaviour should
  add a fixture (`tests/fixtures/*.eml`) + regenerated snapshot.
- **No scope creep:** InLook is a viewer. Don't add network, sending, or account
  features. Suggest improvements, but keep the safe-by-default posture.

## 5. Releasing (automated)

### 5.1 Versioning (SemVer)

InLook versions follow **[Semantic Versioning 2.0.0](https://semver.org/)**:
`MAJOR.MINOR.PATCH`. Because InLook is an *application*, its "public API" — the
contract we promise not to break without a MAJOR bump — is its **user-facing
behaviour**:

- the **CLI surface**: subcommands and flags (`<file.eml>`, no-arg picker,
  `register`, `unregister`, `--version`/`-V`, `--help`/`-h`) and their exit codes;
- the **file association** identifiers Windows + uninstallers rely on: ProgID
  `StruisICT.InLook`, the `.eml`→ProgID mapping, and the WiX MSI `UpgradeCode`
  (which **must never change** — see §6);
- the **package identifiers** downstream channels depend on (e.g. winget
  `StruisICT.InLook`, Flatpak `com.struisict.InLook`).

**Bump rules:**

| Change | Bump | Conventional Commit |
|---|---|---|
| Backwards-**incompatible** change to the contract above (rename/remove a flag, change an exit code, change the ProgID/UpgradeCode/package id, drop a supported input) | **MAJOR** | `feat!:` / `fix!:` or a `BREAKING CHANGE:` footer |
| Backwards-**compatible** new capability (new flag, attachment saving, new platform/package) | **MINOR** | `feat:` |
| Backwards-compatible bug fix or internal change with no behaviour change (render fixes, dependency bumps) | **PATCH** | `fix:` |
| No release on its own | — | `chore:`, `docs:`, `ci:`, `refactor:`, `test:`, `build:`, `style:` |

**Pre-1.0 clause (we are here, at 0.x).** Per SemVer §4, `0.y.z` is *initial
development* — the contract may still change. release-please is configured with
`bump-minor-pre-major: true`, so while in 0.x a **breaking change bumps the
MINOR** (e.g. `0.5.0 → 0.6.0`) instead of jumping to `1.0.0`. Features stay
MINOR, fixes stay PATCH. **Cut `1.0.0` deliberately** — only once the CLI and
file-association contract above is considered stable.

**Pre-releases** use SemVer identifiers and have *lower* precedence than the
finished version: `1.0.0-alpha.1` < `1.0.0-beta.1` < `1.0.0-rc.1` < `1.0.0`.
Do **not** use SemVer build metadata (`+...`); release-please/Cargo tags are
plain `vMAJOR.MINOR.PATCH`.

**Version source of truth:** `.release-please-manifest.json` and the `version`
field in `Cargo.toml` (both bumped by the release PR). Git tags are `vX.Y.Z`.
Never hand-edit either — let release-please do it.

### 5.2 Release flow

Release is driven by **release-please** + GitHub Actions — do **not** hand-edit
versions or `CHANGELOG.md`.

1. Land Conventional-Commit PRs on `main`.
2. `release-please` keeps an open "release PR" (`chore: release X.Y.Z`) with the
   bumped version (`Cargo.toml`, manifest) and generated changelog.
3. **Merging that release PR** tags the version and creates the GitHub Release.
4. `.github/workflows/release.yml` then builds and attaches per-platform
   artifacts:
   - **Windows:** `cargo build --release` → `cargo wix` (MSI) + raw `inlook.exe`.
   - **Linux:** `cargo deb` (.deb) + `scripts/build-appimage.sh` (AppImage).
   - **macOS:** build `aarch64` + `x86_64`, `lipo` into a universal binary,
     `scripts/build-dmg.sh` → `.dmg`.
5. `.github/workflows/packagers.yml` updates downstream package manifests on
   release.

(Version source of truth is covered in §5.1 above.)

## 6. Packaging files (source of truth lives in this repo)

| Path | What |
|---|---|
| `.cargo/config.toml` | Pins `+crt-static` (MSVC) so the EXE statically links the VC++ runtime and launches on a clean Windows install. **Keep it** — removing it reintroduces the `0xC0000135` crash. |
| `build.rs` + `winresource` build-dep | Windows-only: embeds `assets/inlook.ico` + version metadata (ProductName/Company/FileVersion) into `inlook.exe`. |
| `wix/main.wxs` | Windows MSI manifest. **`UpgradeCode` must never change** (keeps upgrades in-place). Version comes from `$(var.Version)`. Icon comes from `assets/inlook.ico` (never the EXE). |
| `packaging/signpath/README.md` | **SignPath Foundation code signing** (the active signing route): free OSS Authenticode cert, wired into `release.yml` behind the `SIGNPATH_ORGANIZATION_ID` var + `SIGNPATH_API_TOKEN` secret; every release needs a manual approval in the SignPath portal. The README's "Code signing policy" section is a Foundation requirement — keep it accurate. |
| `scripts/sign-windows.ps1` | Legacy PFX signing fallback. No-op unless secrets `WINDOWS_CERT_PFX_BASE64` + `WINDOWS_CERT_PASSWORD` are set (newly issued certs no longer come as PFX — SignPath above is the real route). |
| `Cargo.toml` `[package.metadata.deb]` | Debian `.deb` config (incl. hicolor icon assets). |
| `scripts/build-appimage.sh`, `scripts/build-dmg.sh`, `scripts/generate-icons.py` | Linux AppImage / macOS dmg builders; icon-set generator. |
| `assets/` | `inlook.ico` (Windows), `inlook.png` + `icons/inlook-*.png` (Linux hicolor), `inlook.desktop`, `Info.plist` (macOS). |
| `packaging/winget/` | winget submission notes + validated reference manifest (`StruisICT.InLook`). See its `README.md` and the PR #379422 post-mortem. |
| `packaging/flatpak/` | Flathub submission (`com.struisict.InLook.*`). See its own `README.md`; regenerate `generated-sources.json` whenever `Cargo.lock` changes. |
| `packaging/homebrew/inlook.rb` | Homebrew cask source of truth. |

## 7. Dependencies & upgrade policy

Core: `mail-parser` (MIME parsing), `tao` (window), `wry` (WebView), `rfd`
(file/message dialogs), `html-escape`. Windows-only: `windows`, `windows-registry`.

Dependabot (`.github/dependabot.yml`) opens **patch/minor** bumps weekly
(grouped). **Major** bumps of `wry` and `windows-registry` are *ignored* by
Dependabot on purpose — their builder/error APIs changed in 0.5x/0.6x and need a
single coordinated manual upgrade PR with code adaptation + a manual window/render
smoke test.

## 8. Current state (update this section as work lands)

- **Version:** 0.5.0.
- **Working branches of note:** `feat/struisict-org-urls` (org URL rebrand, PR
  #30, unmerged) and `test/render-fuzz-and-snapshots` (lib/bin split + golden
  snapshot tests + cargo-fuzz target, stacked on #30). Several dependabot
  branches open, including `mail-parser 0.11.4` and `tao 0.35.3` (the `tao`
  jump is multi-major — review the window/event-loop API before merging).
- **Repo recently transferred** from `Struis112/InLook` to the `StruisICT` org;
  URLs are being updated to match.

## 9. Roadmap / ideas (not yet built)

Prioritised, viewer-appropriate features:
1. **Save / open attachments** — currently only *listed*. Highest user value;
   fits the existing render path. Mind the safe-by-default posture (confirm
   before writing, sanitise filenames).
2. **"View raw source" / full-headers toggle** — show all headers / raw RFC 822.
3. **Plain-text ↔ HTML toggle** when both parts exist.
4. **Inline `cid:` images** — map `cid:` references to embedded parts as `data:`
   URIs (keeps the no-remote guarantee).
5. **Drag-and-drop** an `.eml` onto the window; open multiple files.

When you pick one up, add a test, follow the commit convention, and update
sections 8–9 here.
