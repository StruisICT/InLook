# SignPath Foundation code signing

> **STATUS — NOT ACTIVE (as of 2026-08).** The SignPath Foundation OSS
> application was **declined on 2026-07-29**: the Foundation program requires
> more established public-trust/visibility signals (GitHub stars, external
> articles, community discussion, institutional backing) than the project has
> today. It was explicitly *not* a quality judgment.
>
> **Windows release binaries currently ship UNSIGNED.** The pipeline below is
> wired but dormant — it activates only when the `SIGNPATH_ORGANIZATION_ID`
> variable and `SIGNPATH_API_TOKEN` secret are set. This document is retained
> for a **future reapplication** (once the project has broader recognition) or
> a **paid SignPath subscription** (<https://docs.signpath.io/change-subscription>).
> Until then, the integrity guarantee is the per-release **SLSA build-provenance
> attestation + CycloneDX SBOM** (see the README's "Build integrity & code
> signing" section).

Once active, InLook's Windows release binaries would be Authenticode-signed
through [SignPath Foundation](https://signpath.org/), which provides free
code-signing certificates to open-source projects. This replaces the
PFX-secrets approach (`scripts/sign-windows.ps1` remains as a no-op fallback) —
CA/B rules since 2023 require private keys in HSMs, so plain `.pfx` certs are no
longer issued.

## Why

The 0.5.0 winget submission was rejected on a Defender heuristic
(`Trojan:Win32/Sprisky.U!cl`) against the unsigned Rust MSI. 0.6.0 scanned
clean, but a trusted signature is the durable fix for Defender heuristics and
SmartScreen reputation. See `packaging/winget/README.md` for the post-mortem.

## How the pipeline works (wired, dormant until configured)

`release.yml` → `build-windows`:

1. Build `inlook.exe` + MSI as before.
2. If the `SIGNPATH_ORGANIZATION_ID` **variable** and `SIGNPATH_API_TOKEN`
   **secret** are set: stage both files (MSI renamed to the stable name
   `inlook.msi`), upload as a GitHub artifact, and submit a signing request via
   `signpath/github-action-submit-signing-request@v2`.
3. **An Approver manually approves the request in the SignPath portal**
   (Foundation requirement — every release). The job waits up to 1 h; if it
   times out, approve and re-run the job.
4. Signed files replace the unsigned ones and are attached to the GitHub
   Release. **Without the variable/secret the release ships unsigned** — which
   is the current state.

## One-time setup (if/when reapplying or going paid)

### 1. Apply (human step — the maintainer submits this)

Apply at <https://signpath.org/apply>. Answers prepared:

| Field | Value |
|---|---|
| Project name | InLook |
| Project URL / repo | <https://github.com/StruisICT/InLook> |
| License | MIT OR Apache-2.0 (both OSI-approved, no dual-license commercial edition) |
| Download page | <https://github.com/StruisICT/InLook/releases> |
| What it does | Small, fast, safe viewer for `.eml`/`.msg` email files. No accounts, no network, no sending. |
| Platform / artifact | Windows: `inlook.exe` + MSI, built by GitHub Actions from source |
| Team | Struis112 (author, reviewer, approver — single maintainer) |

Eligibility checklist:
- [x] OSI license, no proprietary components, no dual licensing
- [x] Actively maintained, already released in signable form
- [x] Functionality documented on the download page (README)
- [ ] **Code signing policy re-published in the README** — the old
      "Code signing policy" section (committers/approvers, SignPath
      attribution) was **removed in PR #97** once the application was declined,
      since it stated signing that isn't happening. The Foundation requires it
      visible on the download page, so **re-add it before reapplying.**
- [x] Binaries built from source verifiably (GitHub-hosted runners only)
- [x] Version metadata embedded in the EXE (`build.rs`/winresource) and MSI
- [ ] **MFA enabled on the GitHub account** — verify before applying
      (required for all team members, on GitHub and SignPath)
- [ ] **More public traction than at the 2026-07 attempt** — stars, external
      references, sustained engagement (the stated reason for the decline)

### 2. Configure the SignPath project (after approval)

In the SignPath portal:

- Connect the **GitHub.com trusted build system** and link the
  `StruisICT/InLook` repository; install the SignPath GitHub App.
- Create project **`inlook`** with signing policy **`release-signing`**
  (manual approval, approver: Struis112). These slugs are hard-coded in
  `release.yml` — change both together.
- Artifact configuration (the artifact arrives as the GitHub-artifact ZIP):

```xml
<artifact-configuration xmlns="http://signpath.io/artifact-configuration/v1">
  <zip-file>
    <pe-file path="inlook.exe">
      <authenticode-sign/>
    </pe-file>
    <msi-file path="inlook.msi">
      <authenticode-sign/>
    </msi-file>
  </zip-file>
</artifact-configuration>
```

### 3. Wire the repo (after approval)

- Repository **variable** `SIGNPATH_ORGANIZATION_ID` — from the portal URL.
- Repository **secret** `SIGNPATH_API_TOKEN` — an API token of a SignPath user
  with Submitter role.

Next release is then signed automatically (modulo the manual approval click).

## Foundation obligations (apply only once signing is active)

These are dormant while the program is inactive; honor them again if a future
application is approved:

- Re-publish and keep the **Code signing policy** section in the README
  accurate (team, roles, privacy statement) — it must stay visible on the
  download page.
- Every team member keeps **MFA** on GitHub + SignPath.
- PRs from non-committers get reviewed before merge (already repo policy).
- Only sign artifacts built by GitHub-hosted runners from this repo.
