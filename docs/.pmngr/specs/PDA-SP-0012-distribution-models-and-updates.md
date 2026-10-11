---
id: PDA-SP-0012
type: spec
title: Distribution, models and updates
status: backlog
author: mcp
labels: [packaging]
created: 2026-10-11T21:41:32Z
updated: 2026-10-11T22:04:42Z
requirements:
  R1:
    status: backlog
  R2:
    status: backlog
  R3:
    status: backlog
  R4:
    status: backlog
---

## Purpose
Install, update and fetch models in a verifiable way and with proper licences on Linux, macOS and Windows.

## Scope
cargo-packager (deb, AppImage, dmg, NSIS/MSI) + Flatpak, signing/notarization, velopack, model manifest with SHA-256 and pinned HF commits, downloads with consent, NOTICE.

Refs: `docs/research/bitacora-gpui.md` §4, `docs/research/voice-stack.md` §6.

## Requirements

### PDA-SP-0012.R1 — Packages per platform

Releases SHALL produce `.deb`, AppImage and Flatpak (Linux x86_64 + aarch64), signed and notarized `.app`/`.dmg` (macOS arm64; x86_64 with fallback STT), and signed NSIS/MSI (Windows x86_64) including the `pando-voice` worker, with smoke tests that install and launch each artifact in CI.

#### Scenario: fresh Ubuntu 24.04
- WHEN the .deb is installed
- THEN Vulkan/audio deps are pulled and the app launches and transcribes offline

### PDA-SP-0012.R2 — Verifiable model manager

Models SHALL be described in an embedded manifest (id, url pinned to Hugging Face commit SHA, sha256, size, licence, attribution). Whistle, Silero VAD and the default voice SHALL be bundled; others downloaded only after explicit consent with progress, resume (`.part` + atomic rename) and SHA-256 verification, into the per-user data dir, overridable with `PANDO_ASSISTANT_MODELS_DIR` and importable offline (`models import <dir>`).

#### Scenario: tampered download
- GIVEN a downloaded file with wrong hash
- THEN it is deleted, the model stays unavailable and the user is told

### PDA-SP-0012.R3 — Automatic updates

The app SHALL check for updates with velopack (initialised first in `main`), download in background and apply on restart with user consent; Flatpak and .deb builds SHALL defer to their package managers.

#### Scenario: update available
- THEN a non-blocking toast offers "Reiniciar para actualizar"

### PDA-SP-0012.R4 — Licences and notices

Every artifact SHALL ship a NOTICE/licence bundle covering Rust deps (generated, `cargo-deny` allow-list blocking GPL in the app binary), needle engine and Whistle weights (Apache-2.0), ONNX Runtime (MIT), voices (CC0/CC-BY with attribution), fonts (OFL) and, for the `pando-voice` worker, espeak-ng (GPL-3) with its source offer.

#### Scenario: CI licence gate
- WHEN a GPL crate enters the app dependency graph
- THEN `cargo deny` fails the build
