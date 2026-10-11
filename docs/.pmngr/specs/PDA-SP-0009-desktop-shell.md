---
id: PDA-SP-0009
type: spec
title: Desktop shell
status: backlog
author: mcp
labels: [shell]
created: 2026-10-11T21:41:32Z
updated: 2026-10-11T22:04:29Z
requirements:
  R1:
    status: backlog
  R2:
    status: backlog
  R3:
    status: backlog
  R4:
    status: backlog
  R5:
    status: backlog
  R6:
    status: backlog
  R7:
    status: backlog
  R8:
    status: backlog
---

## Purpose
Everything that turns the app into a desktop assistant: windows, mini window, global hotkey, tray, settings, notifications, language, accessibility.

## Scope
GPUI via `gpui-kit =0.7.1` (`ui/` facade), main window (organism + chat), mini window 380×440, `global-hotkey` + `ashpd` GlobalShortcuts portal + `ptt` CLI, tray `ksni`/`tray-icon`, `auto-launch`, atomic JSON settings + keychain, `rust-i18n` es/en, OS reduced motion, single instance.

Refs: `docs/research/bitacora-gpui.md` §3, `docs/research/voice-stack.md` §5.

## Requirements

### PDA-SP-0009.R1 — Main window: organism + chat

The main window SHALL combine the organism canvas (header with wordmark, project pill, mono status chips `stt · …`, `tts · …`, `decisión · …`, `● local`; state label, hint, mic button, legend) and the chat client in a layout switchable between Orbe, Chat and Dividido, persisted per user. It SHALL be frameless with client decorations where granted, using brand tokens (Bosque background, Marfil text at alpha, Álamo accents) and bundled fonts (Space Grotesk, JetBrains Mono, Zen Kaku Gothic New).

#### Scenario: layout persistence
- WHEN the user switches to Dividido and restarts
- THEN the window opens in Dividido

### PDA-SP-0009.R2 — Floating mini window

A floating mini window (380×440, radius 32, transparent background) SHALL show the same organism at 118 px with state and last transcript and the hotkey hint, stay always-on-top where the platform allows (macOS/Windows `PopUp`, X11 PopUp with own drag, Wayland `LayerShell` on wlroots/KDE, normal window on GNOME), throttle frames when inactive, and expand the main window on click.

#### Scenario: GNOME Wayland
- THEN the mini window opens as a normal window and the docs explain the limitation

### PDA-SP-0009.R3 — Global push-to-talk hotkey

A configurable global hold-to-talk shortcut SHALL work while the app is unfocused: `global-hotkey` on Windows, macOS and X11 (Pressed/Released), the XDG GlobalShortcuts portal via `ashpd` on Wayland (Activated/Deactivated), and a single-instance IPC CLI `pando-assistant ptt start|stop|toggle` as fallback. Defaults SHALL avoid OS conflicts (no bare ⌥Space on GNOME/KDE/Windows).

#### Scenario: Sway
- GIVEN no GlobalShortcuts portal
- THEN settings show the exact `bindsym` lines for the CLI fallback

### PDA-SP-0009.R4 — Tray and background running

A tray icon (`ksni` on Linux, `tray-icon` on macOS/Windows) SHALL mirror the body state and pending count and offer: Escuchar, Manos libres on/off, Silenciar voz, Abrir ventana, Mini ventana, Salir. Closing the main window SHALL keep the app running in background when the tray is available.

#### Scenario: close window
- WHEN the user closes the main window with tray available
- THEN the process keeps running and the hotkey still works

### PDA-SP-0009.R5 — Settings and secrets

Settings SHALL persist as tolerant JSON written atomically in the platform config dir (`directories`), secrets only in the OS keychain, covering: Pando URL override, language, STT engine/language, TTS voice/rate/mute, hotkey, hands-free, audio devices/host, layout, reduced motion, autostart, decision model display. Advanced Pando settings SHALL open the Pando web UI instead of being reimplemented.

#### Scenario: corrupt settings
- GIVEN an unparsable settings.json
- THEN the app starts with defaults and keeps a backup of the bad file

### PDA-SP-0009.R6 — Language es/en

All UI copy SHALL come from `rust-i18n` locale files (Spanish source, English second), sharing the language switch with GPUI Kit widgets; system locale picks the default. Spoken phrases of the assistant SHALL follow the same language.

#### Scenario: English
- WHEN the user sets English
- THEN UI and spoken confirmations are in English without restart

### PDA-SP-0009.R7 — System notifications

The app SHALL show toasts from `/api/v1/notifications/stream` and OS notifications when unfocused for: agent waiting for the user, agent finished when the user asked to be notified, and connection lost. Clicking an OS notification SHALL focus the related cell or session.

#### Scenario: agent finished while unfocused
- THEN an OS notification appears and clicking it opens the cell card

### PDA-SP-0009.R8 — Single instance, autostart and accessibility

Only one app instance SHALL run (second launch forwards args via IPC and focuses the window). Autostart SHALL be optional (`auto-launch`; Background portal in Flatpak). The OS reduced-motion setting SHALL be detected (GNOME `enable-animations`, macOS `accessibilityDisplayShouldReduceMotion`, Windows `SPI_GETCLIENTAREAANIMATION`) and applied live; interactive elements SHALL have accessible roles and labels.

#### Scenario: second launch
- WHEN the user starts the app again
- THEN the existing window is focused and no second process stays alive
