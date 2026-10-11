---
id: PDA-T-0168
type: task
title: G13 · Portable instance registry with scheme/base_url
status: cancelled
priority: medium
parent: PDA-US-0049
milestone: PDA-M-0003
author: mcp
labels: [upstream-pando]
estimate: 2
created: 2026-10-11T21:57:00Z
updated: 2026-10-11T22:21:30Z
closed: 2026-10-11T22:21:30Z
---

## Description
Add `scheme`/`base_url` to `instanceregistry.Entry`; use `os.TempDir()` or the user's runtime dir instead of the fixed `/tmp/pando-instances` (`registry.go:14`), keeping read compatibility.

## Acceptance Criteria
- Discovery works on Windows and macOS.
