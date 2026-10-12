# Packaging

Everything that turns the three binaries into installable artifacts. Config is under this
directory; the driver is `cargo xtask` (see `xtask/src/release.rs`).

| Command | What it does |
|---|---|
| `cargo xtask notice` | `dist/legal/`: NOTICE, SOURCE-OFFER, licence texts, per-binary `THIRD-PARTY-*.md` (from `cargo deny list`) and a `cargo deny check licenses` gate per binary graph |
| `cargo xtask check-nm [dir]` | ADR 0007 gate: 0 `espeak_` symbols in `pando-assistant` and `pando-stt` (run before stripping) |
| `cargo xtask source-bundle` | `dist/pando-assistant-<v>-corresponding-source.tar.gz`: the GPL-3 Corresponding Source of `pando-tts` |
| `cargo xtask bundle [--no-build] [--formats deb,appimage]` | release build, gates, stage models and legal files in `dist/stage`, run `cargo packager --config packaging/Packager.toml` into `dist/packages` |

Inputs that are not committed: `vendor/needle/` (`scripts/fetch-needle.sh --model`) and
`vendor/voices/<voice>/` (a sherpa-onnx Piper voice dir, e.g. `es_ES-davefx-medium`, CC0).

Installed layout (deb): `/usr/bin/{pando-assistant,pando-stt,pando-tts}` (workers next to the
app), `/usr/lib/pando-assistant/{models,legal}`. The app must look for the workers beside its
own executable and for models in `../lib/pando-assistant/models` (supervisor task PDA-T-0090).

| Target | Files | Status |
|---|---|---|
| Linux deb + AppImage x86_64 | `Packager.toml`, `linux/` | built locally |
| Linux aarch64 | same config; `cargo build --release --target aarch64-unknown-linux-gnu` on an arm64 host, `scripts/fetch-needle.sh linux-arm64`, `cargo packager --target aarch64-unknown-linux-gnu` | config only, not built |
| Flatpak | `flatpak/` | manifest + metainfo validated, not built |
| macOS | `macos/` (Info.plist, entitlements, es/en strings, `sign-notarize.sh`) | needs a Mac and Apple credentials |
| Windows | `windows/` (`Packager.toml` `[nsis]`/`[wix]`, `sign.ps1`) | needs Windows and a certificate |

## Local build notes

- With AppImageLauncher installed (binfmt interception) the AppImage step hangs on its integration
  dialog. Zero the AppImage magic bytes of the cached tools once, then build with
  `APPIMAGELAUNCHER_DISABLE=1`:
  `dd if=/dev/zero bs=1 count=3 seek=8 conv=notrunc of=~/.cache/.cargo-packager/AppImage/linuxdeploy-plugin-appimage.AppImage`
  (cargo-packager already does it for `linuxdeploy-x86_64.AppImage`).
- Running an AppImage on a host without FUSE: `APPIMAGE_EXTRACT_AND_RUN=1`.
- `flatpak-builder` was not installed on the dev machine, so the Flatpak manifest was only
  validated structurally (YAML, `appstreamcli validate` on the metainfo).
