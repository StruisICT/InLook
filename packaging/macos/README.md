# macOS signing & notarization

InLook's `.dmg` should be **Developer ID-signed and notarized** so it opens on
current macOS without Gatekeeper warnings. Since macOS Catalina (and enforced
harder on Apple Silicon and macOS 15+/Tahoe), an unsigned/un-notarized app
downloaded from the internet is blocked — users see *"InLook is damaged and
can't be opened"* or *"cannot be verified"*.

The pipeline is already wired into `release.yml` and `scripts/build-dmg.sh`,
gated on secrets/variables — exactly like the Windows SignPath setup. **Until
you add the credentials below, releases ship the `.dmg` unsigned**, unchanged.

## What the pipeline does (once configured)

1. Imports your Developer ID Application certificate into a temporary keychain.
2. `scripts/build-dmg.sh` code-signs the binary and the `.app` with the
   **hardened runtime** + `assets/macos/entitlements.plist` (only
   `com.apple.security.cs.allow-jit`, for WKWebView), then signs the `.dmg`.
3. `xcrun notarytool submit --wait` sends the `.dmg` to Apple's notary service
   and blocks on the result (a rejection fails the release job).
4. `xcrun stapler staple` attaches the notarization ticket so the `.dmg`
   validates **offline** on first launch.
5. The existing SLSA attestation + release upload then run on the stapled dmg.

## One-time setup (requires a paid Apple Developer account)

### 1. Enroll & create the signing certificate

- Join the [Apple Developer Program](https://developer.apple.com/programs/)
  (~US$99/year). Note your **Team ID** (Membership details).
- Create a **Developer ID Application** certificate
  (Certificates, IDs & Profiles → **+** → *Developer ID Application*). Install
  it, then export it from Keychain Access as a `.p12` with a password.
- Your **signing identity** string is what `security find-identity -p codesigning`
  prints, e.g. `Developer ID Application: Struis ICT (ABCDE12345)`.

### 2. Create an App Store Connect API key (for notarytool)

- In [App Store Connect → Users and Access → Integrations → App Store Connect
  API](https://appstoreconnect.apple.com/access/integrations/api), create a key
  with the **Developer** role. Download the `.p8` (you can only download once).
- Note the **Key ID** and the **Issuer ID** shown on that page.

### 3. Add repository secrets and one variable

Repository **secrets**:

| Name | Value |
|---|---|
| `APPLE_CERT_P12_BASE64` | base64 of the `.p12` (`base64 -i cert.p12 \| pbcopy`) |
| `APPLE_CERT_PASSWORD` | the `.p12` export password |
| `APPLE_API_KEY_P8_BASE64` | base64 of the App Store Connect `.p8` |
| `APPLE_API_KEY_ID` | the API Key ID |
| `APPLE_API_ISSUER_ID` | the API Issuer ID |

Repository **variable**:

| Name | Value |
|---|---|
| `APPLE_SIGNING_IDENTITY` | e.g. `Developer ID Application: Struis ICT (ABCDE12345)` |

The next release then signs, notarizes, and staples automatically.

## Verifying a released build

```sh
# Signature + hardened runtime
codesign --verify --deep --strict --verbose=2 /Applications/InLook.app
spctl --assess --type execute --verbose /Applications/InLook.app   # → accepted
# Notarization ticket is stapled
xcrun stapler validate InLook-*-universal.dmg
```

## Interim: opening the current (unsigned) build

Until notarization is enabled, users on macOS can open the `.dmg`/app with a
one-time bypass:

- **Right-click** (or Control-click) `InLook.app` → **Open** → **Open** in the
  dialog; or
- remove the quarantine attribute:
  ```sh
  xattr -dr com.apple.quarantine /Applications/InLook.app
  ```

This is documented for users in the README and the wiki Installation page.
