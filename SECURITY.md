# Security Policy

InLook is a *safe-by-default* email viewer — opening a hostile `.eml` or `.msg`
should never let it run code, track you, or reach the network. We take security
reports seriously and appreciate coordinated disclosure.

## Reporting a vulnerability

**Please do not open a public issue for security problems.** Use one of these
private channels instead:

1. **Preferred — GitHub private vulnerability reporting.** On
   <https://github.com/StruisICT/InLook>, go to the **Security** tab →
   **Report a vulnerability**. This opens a private advisory visible only to you
   and the maintainers.
2. **Email** — <info@struisict.com> with the subject line `InLook security`.
   If you want to encrypt, say so in a first (non-sensitive) message and we'll
   arrange a key.

Please include, as far as you can:

- the InLook version (`inlook --version`) and your OS;
- a description of the issue and its impact;
- steps to reproduce, and a minimal sample file if the bug is triggered by a
  crafted email (a `.eml`/`.msg` that reproduces it).

## What to expect

- **Acknowledgement** within **3 business days**.
- An initial assessment (severity, whether we can reproduce) within **10
  business days**.
- We will keep you updated on progress and let you know when a fix ships.
- With your permission we will credit you in the release notes and the
  advisory. If you prefer to stay anonymous, that's fine too.
- We ask that you give us a reasonable window to release a fix before any
  public disclosure. We aim to disclose within **90 days** of the report, or
  sooner once a fix is available.

## Supported versions

InLook is pre-1.0 and ships fixes on the latest release line. Security fixes are
made against the **latest released version**; please reproduce on the newest
release before reporting. Older versions do not receive backported fixes while
in `0.x`.

| Version | Supported |
|---|---|
| latest release | ✅ |
| older `0.x` | ❌ (update to the latest release) |

## Scope

In scope — the security posture InLook promises to uphold:

- Parsing and rendering attacker-controlled `.eml` / `.msg` / `.oft` files
  (`src/render.rs`, `src/msg.rs`, `src/extract.rs`): crashes/panics reachable
  from a crafted file, unbounded resource use, or memory-safety issues.
- **HTML/script/content isolation**: any way email content escapes the
  sandboxed iframe or the Content-Security-Policy, injects markup into InLook's
  own UI, loads remote content, or runs a script.
- **Attachment handling**: path traversal via crafted attachment names, or any
  route by which an attachment reaches an auto-execute path.
- **The opt-in update check** (`src/update.rs`, Windows): anything that makes it
  contact the network without consent, mishandles the response, or could be
  abused to mislead the user.

Out of scope — issues in upstream dependencies (report those upstream; we track
advisories via `cargo audit` and Dependabot), and social-engineering or
physical-access attacks.

## How we build and ship (verifying what you run)

- Releases are built from source by GitHub Actions, not a developer machine.
- Windows binaries are **Authenticode-signed** via SignPath Foundation.
- Release artifacts carry **SLSA build provenance attestations**, so you can
  verify a downloaded binary was built by our workflow from this repository:

  ```sh
  gh attestation verify <artifact> --repo StruisICT/InLook
  ```

- A **CycloneDX SBOM** is attached to each release listing the exact dependency
  versions the binaries were built from.

## Our commitments

- No telemetry; offline by default (the only network path is the explicitly
  opt-in update check — see the README privacy policy).
- We run `cargo audit` on CI and fuzz the untrusted-input render path.
- We disclose fixed vulnerabilities in the changelog and, where warranted, as
  GitHub Security Advisories.
