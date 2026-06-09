# winget packaging

Source-of-truth notes for getting **InLook** onto the
[Windows Package Manager Community Repository](https://github.com/microsoft/winget-pkgs)
(`winget`). Package identifier: **`StruisICT.InLook`**.

> **Be a good guest.** winget-pkgs is reviewed by volunteers. Every PR triggers
> a validation + security pipeline and human moderation. Get each submission
> **right the first time** — validate and *install-test* locally before opening
> a PR, and never open a second PR for something already in flight.

## How releases reach winget

| Submission | How | Who does it |
|---|---|---|
| **First version** | **Manual** PR to `microsoft/winget-pkgs` (the bot only updates *existing* packages) | a maintainer, once |
| **Every later version** | **Automatic** via `winget-releaser@v2` in `.github/workflows/packagers.yml`, fired on each GitHub Release | CI |

So this is a one-time manual effort; after `StruisICT.InLook` exists upstream,
new releases self-publish.

## Post-mortem: PR #379422 (first v0.5.0 attempt)

The first submission ([microsoft/winget-pkgs#379422](https://github.com/microsoft/winget-pkgs/pull/379422))
failed validation repeatedly. Lessons, so we never burn maintainer/volunteer
time on these again:

| Symptom in the pipeline | Real cause | Fix (applied) |
|---|---|---|
| `STATUS_DLL_NOT_FOUND` / `0xC0000135` launching `inlook.exe` | The EXE imported **`VCRUNTIME140.dll` + `VCRUNTIME140_1.dll`** (the VC++ runtime), absent on the clean validator image. **Not** WebView2 — `WebView2Loader.dll` is loaded at runtime, not a load-time import. | Statically link the CRT via [`.cargo/config.toml`](../../.cargo/config.toml) (`-C target-feature=+crt-static`). The rebuilt EXE imports only core Windows system DLLs. |
| `APPINSTALLER_CLI_ERROR_INSTALL_MISSING_DEPENDENCY` — "No suitable installer found for Microsoft.EdgeWebView2Runtime" | A `Dependencies.PackageDependencies` block (EdgeWebView2Runtime + VCRedist) was added as a wrong fix for the above; winget then tried and failed to resolve those deps in the sandbox. | **Do not declare those dependencies.** The static-CRT build removes the need, and WebView2 is preinstalled on Win10 22H2+/Win11. |
| Trenly: delete `Scope: machine` | Redundant — winget infers scope for a perMachine MSI; specifying it causes "unnecessary mapping in the CLI". | Removed `Scope`. |
| Trenly: delete `InstallerType` inside `AppsAndFeaturesEntries` | The whole `AppsAndFeaturesEntries` block was redundant (DisplayVersion == PackageVersion, ProductCode already at installer level, names match the locale). | Dropped the `AppsAndFeaturesEntries` block entirely. |
| `[FAIL] Installer failed security check ... Trojan:Win32/Sprisky.U!cl` | **Defender false positive** on the unsigned Rust MSI. Per [Policies](https://github.com/microsoft/winget-pkgs/blob/master/doc/Policies.md#security-scans-and-potentially-unwanted-applications-pua), *any* security-scan hit is an automatic rejection. | **Open blocker — see below.** Release pipeline now Authenticode-signs the EXE + MSI when a cert is configured. |
| The validator also ran `C:\Windows\Installer\{...}\ProductIcon.exe` and it crashed | The MSI used the **EXE itself as its ARP icon** (`<Icon SourceFile='...inlook.exe'>`), so it embedded a second copy of the binary that Windows extracted and the validator executed. | WiX now points the icon at `assets/inlook.ico`; `build.rs` (winresource) embeds the icon + version metadata into the EXE. Smaller MSI, no second executable. |

> **Do not resubmit v0.5.0.** The fixes above require a *rebuilt* MSI, so the
> next winget submission must target a new release (the static-CRT `fix:` lands
> as `0.5.1`/next) with a freshly computed `InstallerUrl`, `InstallerSha256`,
> and `ProductCode`. Let the stale PR #379422 close on its own.

## Open blocker: Defender false positive (`Trojan:Win32/Sprisky.U!cl`)

This is the one issue that cannot be fixed by editing the manifest, and it will
block acceptance until cleared. Action plan, in order:

1. **Rebuild and re-scan.** The static-CRT binary is a different PE; scan the new
   `.msi` and `.exe` with an up-to-date Defender (`MpCmdRun.exe -Scan -ScanType 3
   -File <path>`). The heuristic may no longer trigger.
2. **Submit a false-positive report** to Microsoft as the software developer:
   <https://www.microsoft.com/en-us/wdsi/filesubmission> (choose "Software
   developer", attach the MSI + EXE, link the public source repo). Turnaround is
   usually 1–3 days; once cleared, re-run validation with `@wingetbot run`.
3. **Code-sign the binary** (the durable fix, now wired up). The release
   workflow signs `inlook.exe` *and* the `.msi` via
   [`scripts/sign-windows.ps1`](../../scripts/sign-windows.ps1). To activate it,
   add two repo secrets:
   - `WINDOWS_CERT_PFX_BASE64` — base64 of your code-signing `.pfx`
     (`base64 -w0 cert.pfx` / `[Convert]::ToBase64String([IO.File]::ReadAllBytes('cert.pfx'))`).
   - `WINDOWS_CERT_PASSWORD` — the PFX password.

   An OV/EV Authenticode cert massively reduces heuristic FPs and builds
   SmartScreen reputation. Without the secrets the build still succeeds but ships
   unsigned (and will keep failing winget's security gate).

Until the binary scans clean (ideally signed), do **not** reopen a winget PR —
it will just fail the security gate again.

## Build requirement: static CRT

`.cargo/config.toml` pins `+crt-static` for the MSVC target. Keep it. Verify any
release EXE is self-contained before shipping:

```sh
# Should list only core Windows DLLs — NO vcruntime140*.dll, NO api-ms-win-crt-*
grep -aoE '[A-Za-z0-9_-]+\.dll' target/release/inlook.exe | sort -u
```

## One-time prerequisites

- [ ] The GitHub account that opens the PR has signed the
      [Microsoft CLA](https://cla.opensource.microsoft.com) (one-time, all MS repos).
- [ ] A fork of `microsoft/winget-pkgs` exists under that account
      (currently `Struis112/winget-pkgs`).
- [ ] Repo **secret** `PACKAGERS_TOKEN` — a PAT (classic, `public_repo`) owned by
      that same account, so the CI bot can push the fork branch + open the PR.
- [ ] Repo **variable** `ENABLE_PACKAGERS=true` — master switch for
      `packagers.yml`.

## Repository rules we must satisfy (learned from their docs)

- **Installer type:** MSIX / MSI / exe / font only. We ship an **MSI** ✅
  (scripts like `.ps1`/`.bat` are banned).
- **Silent install:** must complete unattended. Our perMachine MSI installs with
  `/qn` ✅.
- **Stable, version-specific URL** from the official source: the GitHub release
  asset `inlook-<version>-x86_64.msi` ✅ (unique URL per version — avoids
  hash-mismatch churn).
- **Multi-file** manifest (version + defaultLocale + installer). Singleton
  manifests are **not allowed** ✅.
- **Latest schema** (`ManifestVersion: 1.12.0`) with a
  `# yaml-language-server: $schema=...` header on every file ✅.
- **One PR = one package version, manifest files only.** No README/doc/tooling
  changes mixed in; no two versions in one PR.
- **No PUA / clean security scan** — InLook does no network I/O ✅.

## Directory layout (in the winget-pkgs fork)

```
manifests/s/StruisICT/InLook/<version>/
  StruisICT.InLook.yaml              # version
  StruisICT.InLook.installer.yaml    # installer (URL, SHA256, ProductCode)
  StruisICT.InLook.locale.en-US.yaml # defaultLocale (metadata)
```

A validated reference set for **0.5.0** lives in
[`reference/`](reference) next to this file. It is a **snapshot for reference
only** — `winget-releaser` regenerates the real manifests on release, so do not
hand-maintain it version to version.

## Per-version values to (re)compute

The installer manifest needs values pulled from the *actual released MSI*:

- `InstallerSha256` — `sha256sum inlook-<v>-x86_64.msi` (upper-case).
- `ProductCode` — changes every build (WiX `Product Id='*'`). Extract it:
  ```powershell
  $i = New-Object -ComObject WindowsInstaller.Installer
  $db = $i.GetType().InvokeMember('OpenDatabase','InvokeMethod',$null,$i,@('C:\path\inlook.msi',0))
  $v = $db.GetType().InvokeMember('OpenView','InvokeMethod',$null,$db,@('SELECT Value FROM Property WHERE Property=''ProductCode'''))
  $v.GetType().InvokeMember('Execute','InvokeMethod',$null,$v,$null)
  $r = $v.GetType().InvokeMember('Fetch','InvokeMethod',$null,$v,$null)
  $r.GetType().InvokeMember('StringData','GetProperty',$null,$r,1)
  ```
- `ReleaseDate` — the GitHub release's published date (`YYYY-MM-DD`).
- The `UpgradeCode` is fixed in `wix/main.wxs` and must never change.

`winget-releaser` / `wingetcreate` extract all of these from the MSI
automatically — prefer them over doing it by hand.

## Pre-submission checklist (do all of these)

1. `winget validate --manifest <dir>` → **Manifest validation succeeded.**
2. Install-test locally (elevated):
   ```powershell
   winget settings --enable LocalManifestFiles
   winget install --manifest <dir>
   ```
   …or, cleaner, in **Windows Sandbox** with the repo's `Tools\SandboxTest.ps1`.
3. Confirm there is **no open PR** already for this package/version.
4. PR title: `New package: StruisICT.InLook version <X.Y.Z>` (first time) or
   `Update: StruisICT.InLook to <X.Y.Z>`.
5. PR contains **only** the one version's manifest set — nothing else.
6. The release EXE is self-contained (no `vcruntime140*.dll` import — see
   "Build requirement" above).
7. The `.msi` and `.exe` scan **clean** with current Defender, and ideally are
   **code-signed**, so the security gate passes.

## References

- Authoring: <https://github.com/microsoft/winget-pkgs/blob/master/doc/Authoring.md>
- First-time checklist: <https://github.com/microsoft/winget-pkgs/blob/master/doc/FirstContribution.md>
- Policies: <https://github.com/microsoft/winget-pkgs/blob/master/doc/Policies.md>
- 1.12 schema: <https://github.com/microsoft/winget-pkgs/tree/master/doc/manifest/schema/1.12.0>
- Validation failures: <https://github.com/microsoft/winget-pkgs/blob/master/doc/ValidationFailureGuide.md>
