---
id: PDA-T-0152
type: task
title: Tag-triggered release workflow
status: backlog
parent: PDA-US-0045
milestone: PDA-M-0011
author: mcp
labels: [packaging, ci]
estimate: 3
created: 2026-10-11T21:56:03Z
updated: 2026-10-11T22:03:01Z
---

## Description
`release.yml`: check tag == workspace version, build all artifacts (Linux x86_64/aarch64, signed macOS, signed Windows, Flatpak), sha256sums, release notes, velopack feed.

## Acceptance Criteria
- Release candidate published as draft without manual steps.
