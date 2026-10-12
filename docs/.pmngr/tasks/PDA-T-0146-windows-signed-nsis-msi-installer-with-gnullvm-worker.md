---
id: PDA-T-0146
type: task
title: "Windows: signed NSIS/MSI installer with gnullvm worker"
status: in_review
parent: PDA-US-0043
milestone: PDA-M-0011
assignees: [claude-sonnet]
author: mcp
labels: [packaging]
estimate: 3
created: 2026-10-11T21:56:03Z
updated: 2026-10-12T08:15:14Z
started: 2026-10-12T08:01:03Z
---

## Description
NSIS `install-mode both`, es/en languages, embedded icon (`embed-resource`), signing (thumbprint or Azure Trusted Signing), include `pando-voice.exe` + the required llvm-mingw runtime.

## Acceptance Criteria
- SmartScreen does not block when signed; clean uninstall.
