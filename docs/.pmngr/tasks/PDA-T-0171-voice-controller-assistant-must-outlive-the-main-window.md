---
id: PDA-T-0171
type: task
title: Voice controller/assistant must outlive the main window (background mode PTT)
status: backlog
priority: high
parent: PDA-US-0033
author: mcp
labels: [voice-in, shell, bug]
created: 2026-10-12T08:58:35Z
updated: 2026-10-12T08:58:35Z
---

## Description
VoiceController, Assistant, ClaudeBridge and the voice settings sections are created inside MainView::new. In background/tray mode (main window removed) the shell global hotkey, `pando-assistant ptt` and tray PTT/hands-free reach `voice::controller::ptt_edge`, which silently drops the edge because the weak handle is dead. Settings opened from the tray also lack the Voice/Audio sections until a main window was opened once.

## Acceptance Criteria
- Voice entities are owned at app level (or recreated headless) so PTT, hands-free and mute work with no main window.
- Settings opened from the tray always show Voice/Audio sections.
- Test: shell output with no main window still reaches the controller.
