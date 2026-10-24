# Release pipeline

`.github/workflows/release.yml` builds, signs and publishes a **draft** GitHub release when a
`v*` tag is pushed (or by hand: `gh workflow run release.yml -f tag=vX.Y.Z`). It uses the Digio
signing scheme shared with `digiogithub/bitacora` and `digiogithub/pando`. Packaging itself is
`cargo xtask bundle` (see `packaging/README.md`); the workflow wraps it per OS.

## Jobs

| Job | Runner | Output (`dist-*` artifact) |
|---|---|---|
| `meta` | ubuntu-24.04 | Fails unless the tag equals the workspace version **and** `packaging/Packager.toml`'s version, and `RUST_TOOLCHAIN` matches `rust-toolchain.toml`. `-*` tags are pre-releases. |
| `linux` | ubuntu-24.04 | `.deb`, `.AppImage` (x86_64, unsigned) and `pando-assistant-<v>-corresponding-source.tar.gz` (GPL-3 source of `pando-tts`, ADR 0007). |
| `macos` | macos-latest (arm64) | `Pando-Assistant-<v>-aarch64.dmg`: Developer ID signed, hardened runtime, notarized, stapled. No x86_64: there is no macOS x86_64 Whistle engine. |
| `windows` | windows-latest | NSIS `.exe` and (stable tags only) `.msi`, plus the three binaries inside, Authenticode-signed. |
| `release` | ubuntu-24.04 | Downloads only `dist-*`, writes `SHA256SUMS`, creates a draft release. |

All jobs build `-p app -p pando-stt -p pando-tts` in release mode with the toolchain pinned in
`rust-toolchain.toml`, fetch `libneedle.a` and `whistle.cact` with `scripts/fetch-needle.sh`, and
run the ADR 0007 gates through `cargo xtask bundle` (`check-nm` where `nm` applies, `cargo deny`
licences, NOTICE/legal tree).

### Platform specifics

- **Linux**: system packages for GPUI (fontconfig, freetype, wayland, xkbcommon, xcb), cpal
  (ALSA, PipeWire with bindgen/libclang), AppImage (`libfuse2t64`, `file`) and static
  **libc++ 21** from apt.llvm.org, which `needle-sys` links (`/usr/lib/llvm-21/lib`).
- **macOS**: cargo-packager builds `Pando Assistant.app` but signs only the main executable;
  the worker binaries it copies into `Contents/MacOS` stay ad-hoc (linker-signed), which failed
  the v0.1.0 run. The workflow therefore adds the `es`/`en` `InfoPlist.strings` and signs the
  bundle inside-out itself with the Developer ID identity, the hardened runtime and a secure
  timestamp: every Mach-O outside `Contents/MacOS` (dylibs, `Contents/Frameworks`) and any
  `.framework` first, then the helper executables (`pando-stt`, `pando-tts`) with
  `packaging/macos/entitlements.plist` (`com.apple.security.device.audio-input`,
  `com.apple.security.network.client`), then the outer bundle without `--deep`.
  `Info.plist` carries `NSMicrophoneUsageDescription`. The verify step checks `--deep --strict`,
  then the runtime flag, the Developer ID authority and the timestamp on the bundle and on every
  nested Mach-O/framework, and the audio-input entitlement on the app and `pando-stt`. It then
  builds and signs the `.dmg` with `hdiutil`/`codesign`, notarizes it (`macos-notarize`,
  stapled) and checks `xcrun stapler validate`. The keychain is always removed (`if: always()`).
- **Windows**: `pando-assistant.exe` and `pando-tts.exe` are MSVC; `pando-stt.exe` is built for
  `x86_64-pc-windows-gnullvm` with llvm-mingw (pinned by `LLVM_MINGW_VERSION`) because
  `libneedle.a` is GNU ABI, then copied into `target\release`. The three `.exe` are signed
  **before** packaging, then the installers are signed, then `Get-AuthenticodeSignature` must
  report `Valid` for every file. WiX needs numeric versions, so pre-release tags ship NSIS only.

`packaging/macos/sign-notarize.sh` and `packaging/windows/sign.ps1` remain as manual fallbacks;
CI does not use them.

## Signing material

| Platform | Where | What |
|---|---|---|
| macOS | repository secret `MACOS_SIGNING_BUNDLE` | base64 `.tar.gz` of `DIGIO_Software_Signing_Keys` (Developer ID `.p12` + `kvagerc`), unpacked by `digiogithub/ci-actions/macos-signing-keychain@v1` into keychain `pando-assistant-build`, notary profile `pando-assistant-notary`. |
| Windows | environment `release` + repository variables `AZURE_CLIENT_ID`, `AZURE_TENANT_ID`, `AZURE_SUBSCRIPTION_ID`, `AZURE_SIGNING_ENDPOINT`, `AZURE_SIGNING_ACCOUNT`, `AZURE_SIGNING_CERT_PROFILE` | No secret: OIDC federation (`repo:digiogithub/pando-desktop-assistant:environment:release`) into the shared Entra app, then `azure/trusted-signing-action@v2.0.0`. |

Only the `windows` job runs in the `release` environment, so it is the only job that can sign.
Unsigned intermediates never use the `dist-` prefix, so they cannot reach the release.

## Cutting a release

1. Bump `version` in `Cargo.toml` (`[workspace.package]`) and `packaging/Packager.toml`, commit.
2. `git tag vX.Y.Z && git push origin vX.Y.Z` (try `vX.Y.Z-rc.1` first). A tag build uses the
   workflow of the **tagged commit**.
3. Review the draft release (assets, `SHA256SUMS`, notes) and publish it by hand.

## Not verified yet

The workflow passes `actionlint` and YAML parsing but has not run on GitHub. Expected first-run
risks: the Windows `gnullvm` link of `pando-stt` (llvm-mingw + `libneedle.a`, never built before),
the prebuilt static sherpa-onnx on Windows MSVC and macOS, and the installed-models layout on
macOS/Windows (`crates/app/src/voice/resolve.rs` only looks in `<exe>/../lib/pando-assistant/models`).
