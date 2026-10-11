---
id: PDA-T-0006
type: task
title: Tokio↔GPUI bridge and batched event pump
status: done
parent: PDA-US-0002
milestone: PDA-M-0001
assignees: [claude-sonnet]
author: mcp
labels: [shell]
estimate: 2
created: 2026-10-11T21:47:40Z
updated: 2026-10-11T22:42:42Z
started: 2026-10-11T22:42:19Z
closed: 2026-10-11T22:42:42Z
---

## Description
Copy `tokio_bridge.rs` (global runtime, `spawn` with AbortOnDrop, 2 s shutdown) and `events.rs` (`EventPump::attach`, drained in bursts, one `cx.notify()` per batch) with their `#[gpui_test]` tests.

## Acceptance Criteria
- Cancellation tests: dropping the GPUI `Task` aborts the tokio task.
