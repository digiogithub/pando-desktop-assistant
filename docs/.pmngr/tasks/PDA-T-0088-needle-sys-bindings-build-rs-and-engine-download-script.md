---
id: PDA-T-0088
type: task
title: "needle-sys: bindings, build.rs and engine download script"
status: backlog
parent: PDA-US-0026
milestone: PDA-M-0005
author: mcp
labels: [voice-in]
estimate: 3
created: 2026-10-11T21:52:55Z
updated: 2026-10-11T22:02:51Z
---

## Description
Hand-written bindings for `needle.h` (or optional bindgen); `build.rs` locates `vendor/needle/<target>/` (`NEEDLE_DIR` override) and links `libneedle.a` + static libc++/libc++abi (Linux), `c++` (macOS), llvm-mingw (Windows gnullvm); `scripts/fetch-needle.sh` downloads by HF commit SHA and verifies SHA-256.

## Acceptance Criteria
- Clear error message if the engine is missing; Linux/macOS CI links.
