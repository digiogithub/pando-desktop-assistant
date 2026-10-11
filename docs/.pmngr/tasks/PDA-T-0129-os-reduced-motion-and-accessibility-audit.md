---
id: PDA-T-0129
type: task
title: OS reduced motion and accessibility audit
status: backlog
parent: PDA-US-0038
milestone: PDA-M-0008
author: mcp
labels: [shell, a11y]
estimate: 3
created: 2026-10-11T21:54:29Z
updated: 2026-10-11T22:02:28Z
---

## Description
Read GNOME `enable-animations`, macOS `accessibilityDisplayShouldReduceMotion`, Windows `SPI_GETCLIENTAREAANIMATION` → `cx.set_reduce_motion` + live organism; review roles/labels/focus/contrast of the whole UI with a screen reader (Orca, VoiceOver, Narrator).

## Acceptance Criteria
- Changing the OS setting updates the organism without restart; a11y checklist completed.
