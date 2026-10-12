---
created_at: 2026-10-12T08:10:49.05097247Z
updated_at: 2026-10-12T08:10:49.05097247Z
---
# Packaging, NOTICE and user docs (PDA-T-0143/0144/0145/0146/0151/0153)

Builds on [[pando-assistant/plans/backlog-execution_plan.md]], [[adr/0007-espeak-ng-gpl-isolated-tts-worker]] and [[pando-assistant/changes/voice-workers-implementation]].

## What changed
- `xtask/src/release.rs` + `main.rs`: new commands `notice` (per-binary THIRD-PARTY inventories from `cargo deny list`, plus `cargo deny check licenses` gate per graph, NOTICE/licences copied to `dist/legal`), `check-nm` (ADR 0007 gate: no `espeak_` symbols in app / pando-stt), `source-bundle` (GPL-3 Corresponding Source tarball of pando-tts), `bundle` (build, gates, stage models+legal, `cargo packager`). Unit test `count_espeak`.
- `packaging/`: `NOTICE`, `SOURCE-OFFER.md`, `licenses/` (GPL-3, Apache-2.0, CC0, MIT), `Packager.toml` (deb/AppImage/macOS/NSIS/WiX; paths relative to packaging/), `icons/`, `linux/pando-assistant.desktop` (StartupWMClass dev.pando.assistant), `flatpak/` manifest + metainfo, `macos/` (Info.plist, entitlements, es/en InfoPlist.strings, sign-notarize.sh), `windows/` (sign.ps1, README with MSVC/gnullvm split), `README.md`.
- `docs/user/{README,es,en}.md` user documentation.
- `.gitignore`: `/vendor/voices/`, `/dist/`.

## Why
Milestone M10 packaging. Workers ship as sidecars next to the app (`/usr/bin`), models in `/usr/lib/pando-assistant/models`. The app's worker/model discovery (supervisor PDA-T-0090) must match this layout.

## Verification
- `cargo xtask check-nm` on release binaries: app 0, pando-stt 0, pando-tts 59 espeak_ symbols.
- `cargo xtask notice`: `cargo deny check licenses` ok for all three graphs.
- `.deb` built locally (74 MB, x86_64) with 3 binaries, models (Whistle + Piper davefx), legal files; extracted binary runs `--version`. Packaged app binary is the 09:36 build (app crate did not compile at packaging time because of in-progress shell edits).
- metainfo validated with `appstreamcli`, desktop with `desktop-file-validate`, plists parsed. flatpak-builder is not installed, so the manifest was not built.
- Not run: macOS signing/notarization, Windows installers/signing, aarch64 build, Ubuntu 24.04/Debian 12 clean-install test.