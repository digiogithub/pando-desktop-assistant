# Pando Assistant: user guide

Pando Assistant is a native desktop client for [Pando](https://github.com/digiogithub/pando).
It shows your agents as a living organism (the mother cell is Mesnada, every agent is a
daughter cell), includes a complete chat client and is driven by voice. Everything that can
run on your machine does: speech recognition (Whistle), speech synthesis (Piper) and the
model that decides whether you gave a command or are having a conversation.

> Status: parts marked **(planned)** are in the plan but not yet wired into the app. This
> guide describes what exists today.

## 1. Installation

You need a running Pando (`pando serve`, `pando app` or `pando desktop`) on the same machine.
Without Pando you can try the organism with `pando-assistant --demo`.

### Linux (Ubuntu 24.04, Debian 12 or newer)

- **.deb**: `sudo apt install ./pando-assistant_<version>_amd64.deb`
- **AppImage**: `chmod +x Pando_Assistant_*.AppImage && ./Pando_Assistant_*.AppImage`
- **Flatpak**: `flatpak install dev.pando.assistant` (once published).

Requirements: Vulkan (`libvulkan1` and your GPU driver) and PipeWire or PulseAudio for the
microphone. Without a compatible GPU the app falls back to a slower CPU renderer.

### macOS (Apple Silicon, macOS 12 or newer)

Open the `.dmg` and drag the app to Applications. There is no Whistle engine for Intel Macs:
the heavier fallback recogniser (Parakeet) is used instead. **(planned)**

### Windows 10/11 (64-bit)

Run the `.exe` (NSIS) or `.msi` installer. Choose Spanish or English.

## 2. First run

1. Start Pando. The assistant discovers it by itself and shows it in the top bar (connection
   chip). If Pando lives elsewhere, set its address in the settings (`pando_url`).
2. The language follows the system. Force `es` or `en` in `settings.json`.
3. The first time you use the microphone the system asks for permission (macOS, Windows,
   Flatpak). Accept: audio is processed on your machine only.
4. The voice models (Whistle and the Piper voice `es_ES-davefx-medium`) ship inside the
   packages and work offline.

Settings live in `settings.json` in the user's config folder (Linux:
`~/.config/pando-assistant/`). A corrupt file is moved aside to a backup and the app starts
with defaults.

## 3. Shortcuts and windows

| Action | How |
|---|---|
| Talk (hold) | `Ctrl+Alt+Space` (configurable: `shell.ptt_shortcut`) |
| Talk from the compositor (Wayland) | `pando-assistant ptt start` on press and `pando-assistant ptt stop` on release |
| Show / hide the window | `pando-assistant show` / `hide` |
| Floating mini window | `pando-assistant toggle-mini` or the tray menu |
| Quit | `pando-assistant quit` or "Quit" in the tray |

The **system tray** offers Listen, Hands-free, Mute voice, Open window, Mini window and Quit,
plus the number of pending requests. Closing the window keeps the app alive in the tray
(disable with `shell.keep_running`). You can start with your session (`shell.autostart`) and
start minimised (`shell.start_minimized`).

Sway example:

```
bindsym --no-repeat $mod+space exec pando-assistant ptt start
bindsym --release $mod+space exec pando-assistant ptt stop
```

## 4. Voice commands (examples)

Hold the shortcut and speak. The assistant decides whether it is a **command** (executed),
a **conversation** (sent to Pando, answered aloud) or **dictation** (text into the input box:
"dicta...", "apunta...", "toma nota..."). Spanish is the primary language.

- "Cancela migración auth" (cancel an agent).
- "Aprueba" / "Rechaza" when an agent waits for your permission.
- "Cambia al proyecto bitácora".
- "Avísame cuando termine" (announced aloud when a Claude Code session ends).

**Safety**: destructive commands need high confidence plus the literal identifier; when it is
unclear the assistant asks for on-screen confirmation. Anything Pando flags as "requires
explicit approval" is approved only by an explicit decision from you, never automatically.
When in doubt, the assistant does nothing.

## 5. Hands-free and privacy

- **Hands-free** **(planned)**: the wake word "Oye Pando" (or "Hey Pando") starts listening
  without touching the keyboard. Voice detection and verification run locally with Whistle.
- Audio never leaves your machine. Capture, transcription and synthesis are local. The
  assistant only talks to your Pando (normally `localhost`).
- The Pando token is never written to logs.
- The Claude Code integration installs local hooks; it sends no data to any service.
- Logs are in `~/.cache/pando-assistant/logs/` (Linux).

## 6. Claude Code as cells

If you use Claude Code, the assistant can install hooks into its `settings.json` (a backup is
made first; they can be uninstalled). Each session appears as a cell: you see when it works,
when it waits for a permission and when it ends. You can send it messages through the
messaging socket. Known limitation: after you approve a permission the cell stays "waiting"
until the next event.

## 7. Platform limitations

- **Wayland**: there is no universal global shortcut. Use `pando-assistant ptt start/stop` from
  your compositor, or the GlobalShortcuts portal when available **(planned)**. The tray needs
  StatusNotifier (GNOME: the AppIndicator extension).
- **Flatpak**: `/tmp` is not shared, so discovering Pando through `/tmp/pando-instances` falls
  back to the default address.
- **Intel Mac**: no Whistle; heavier fallback recogniser.
- **Windows**: the recogniser worker is a separate executable built with another toolchain;
  seeing three processes is normal.
- Without Vulkan/Metal/DirectX 12 the organism is drawn on the CPU.

## 8. Troubleshooting

| Symptom | What to do |
|---|---|
| "Connection to Pando lost" | Check Pando is still running; the assistant retries by itself. Set `pando_url` if Pando uses another address. |
| Black window or no start | Run `pando-assistant --smoke-test`; try `PANDO_ORGANISM_RENDER=raster`. Read the logs. |
| The microphone is not heard | Check the system input device and the microphone permission. On Linux make sure PipeWire/PulseAudio runs. |
| Speech is slow to start | On PulseAudio the first audio can take about 2 s; PipeWire is faster. |
| The shortcut does nothing (Wayland) | Bind the shortcut in your compositor with `pando-assistant ptt` (section 3). |
| Project names are misheard | Your project names are passed to Whistle as a keyword list; speak clearly and check Pando lists them. |

## 9. Licences

The app and the speech recogniser are MIT. The synthesiser `pando-tts` is GPL-3 (it uses
espeak-ng) and every release offers its complete source. See the `NOTICE` and
`SOURCE-OFFER.md` files shipped in the package.
