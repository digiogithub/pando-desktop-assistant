---
id: PDA-M-0008
type: milestone
title: M7 · Desktop shell
status: backlog
author: mcp
created: 2026-10-11T21:40:56Z
updated: 2026-10-11T22:02:15Z
due: 2027-03-05
---

## Description
Always-on-top mini window (118 px orb), global hotkey (global-hotkey + GlobalShortcuts portal on Wayland + CLI fallback), tray (ksni / tray-icon), settings, system notifications, autostart, i18n es/en, reduced motion.

## Acceptance Criteria
- Global hold-to-talk works on Windows, macOS, X11, KDE 6 and GNOME 48+; fallback documented for Sway.
- Mini window uses ≤ 2% CPU when idle; clicking it expands the main window.
- All UI texts come from the es/en locales.

## Notes
Refs: `docs/research/voice-stack.md` §5, `docs/research/organism-port.md` §3 (mini window).
