# Changelog

## [0.8.0](https://github.com/StruisICT/InLook/compare/v0.7.0...v0.8.0) (2026-07-17)


### Features

* inline cid: images, attachment saving, and nested message opening ([#51](https://github.com/StruisICT/InLook/issues/51)) ([ea67c59](https://github.com/StruisICT/InLook/commit/ea67c595420b3c45118309f67c78272651681ea6))
* view Outlook .msg and .oft email files ([#49](https://github.com/StruisICT/InLook/issues/49)) ([f493f4b](https://github.com/StruisICT/InLook/commit/f493f4bbf89bf81df1bdf31612303a0e199f5217))
* **windows:** Chrome-style default-app flow for register ([#47](https://github.com/StruisICT/InLook/issues/47)) ([3619c45](https://github.com/StruisICT/InLook/commit/3619c4557760c08f3fc6d92f4bfb1d3c712e965b))


### Bug Fixes

* hostile multipart EML could crash InLook — upgrade mail-parser to 0.11 ([#50](https://github.com/StruisICT/InLook/issues/50)) ([a68601d](https://github.com/StruisICT/InLook/commit/a68601d111f41a735e73e633adf83e336548ff33))

## [0.7.0](https://github.com/StruisICT/InLook/compare/v0.6.0...v0.7.0) (2026-07-14)


### Features

* default .eml-app prompt, WebView2 data-folder fix, and new icon ([#43](https://github.com/StruisICT/InLook/issues/43)) ([ac6d904](https://github.com/StruisICT/InLook/commit/ac6d90441b8982ce5382f3b3fd3ff0ac1dddc3e0))

## [0.6.0](https://github.com/StruisICT/InLook/compare/v0.5.0...v0.6.0) (2026-07-10)


### Features

* **ci:** auto-update package manifests on release + flatpak build check ([#27](https://github.com/StruisICT/InLook/issues/27)) ([f901828](https://github.com/StruisICT/InLook/commit/f90182890309e1e4c831039d503f8fe2606c8626))
* rebrand to StruisICT org — static CRT, embedded icon metadata, signed releases ([#30](https://github.com/StruisICT/InLook/issues/30)) ([6f41517](https://github.com/StruisICT/InLook/commit/6f415177f9815808428fa96dfb822da230fef8d1))
* **windows:** embed app icon + version metadata and source the MSI icon from .ico ([6f41517](https://github.com/StruisICT/InLook/commit/6f415177f9815808428fa96dfb822da230fef8d1))


### Bug Fixes

* **flatpak:** pin manifest to v0.5.0 ([#29](https://github.com/StruisICT/InLook/issues/29)) ([50a8ab4](https://github.com/StruisICT/InLook/commit/50a8ab4dc512881ab5e6a77767fbc95d01a85e25))
* statically link the MSVC runtime so InLook starts without VCRedist ([6f41517](https://github.com/StruisICT/InLook/commit/6f415177f9815808428fa96dfb822da230fef8d1))

## [0.5.0](https://github.com/Struis112/InLook/compare/v0.4.0...v0.5.0) (2026-05-25)


### Features

* follow system dark/light theme ([#21](https://github.com/Struis112/InLook/issues/21)) ([5681b38](https://github.com/Struis112/InLook/commit/5681b384ca6102250fb4618c456bf3dfc906fa6e))
* **packaging:** add Flathub submission files ([#25](https://github.com/Struis112/InLook/issues/25)) ([31c2ea3](https://github.com/Struis112/InLook/commit/31c2ea39dff227702a7cad21438d93eae983eab6))
* **packaging:** add Homebrew cask source-of-truth ([#24](https://github.com/Struis112/InLook/issues/24)) ([f0a17f9](https://github.com/Struis112/InLook/commit/f0a17f9d02b26994a72db62d4078341dfeabd501))


### Bug Fixes

* **wix:** use $(var.Version) and update help URL ([#23](https://github.com/Struis112/InLook/issues/23)) ([90b1024](https://github.com/Struis112/InLook/commit/90b1024e01abb3232d8de2f7e3242032f45a810f))

## [0.4.0](https://github.com/Struis112/InLook/compare/v0.3.1...v0.4.0) (2026-05-12)


### Features

* bundle app icon and auto-generate macOS .icns ([#18](https://github.com/Struis112/InLook/issues/18)) ([5d5918a](https://github.com/Struis112/InLook/commit/5d5918a98c43819dd14be3990ee5610766f2e69f))

## [0.3.1](https://github.com/Struis112/InLook/compare/v0.3.0...v0.3.1) (2026-05-12)


### Bug Fixes

* build AppImage on FUSE-less runners ([#16](https://github.com/Struis112/InLook/issues/16)) ([f896aa7](https://github.com/Struis112/InLook/commit/f896aa7112fcd67db664f40ed7746f5264ab6b59))

## [0.3.0](https://github.com/Struis112/InLook/compare/v0.2.0...v0.3.0) (2026-05-12)


### Features

* multi-platform releases (Windows MSI/exe, Linux .deb + AppImage, macOS .dmg) ([#14](https://github.com/Struis112/InLook/issues/14)) ([26ee598](https://github.com/Struis112/InLook/commit/26ee5986c7a0e3ef7abfa1ec02574382941db90c))

## [0.2.0](https://github.com/Struis112/InLook/compare/v0.1.0...v0.2.0) (2026-05-08)


### Features

* harden inputs, add unit tests, security audit, dependabot ([#2](https://github.com/Struis112/InLook/issues/2)) ([c3653d7](https://github.com/Struis112/InLook/commit/c3653d71258706d23076fd6a2718332dc1c6d5c0))
