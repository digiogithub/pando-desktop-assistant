# T-0026 · Global hotkey and tray integrated with the GPUI event loop (spike S6)

Task: PDA-T-0026. Code: `spikes/gpui-shell/src/hotkey.rs` (`gpui-shell-spikes hotkey --backends x11,portal,tray`).

## Architecture that works
Every backend runs **outside** the GPUI thread and only does `async_channel::Sender::try_send(Ev)`; the app side is the
existing `EventPump` shape (a `cx.spawn` loop on the foreground executor draining the receiver). GPUI is never blocked and
no backend needs a GPUI handle. A hold-to-talk state machine (Pressed -> listening, Released -> idle) sits on the GPUI side.

## Results
| Backend | Where tested | Result |
|---|---|---|
| `global-hotkey` 0.7 (X11) | Xvfb + `xdotool keydown/keyup ctrl+alt+p`, GPUI running | `register` OK; **Pressed and Released both delivered**, 2 hold cycles gave `listening,idle,listening,idle`; handler-to-GPUI latency 120-190 us. Uses `GlobalHotKeyEvent::set_event_handler` (no polling thread). |
| XDG `GlobalShortcuts` portal via `ashpd` 0.13 | COSMIC Wayland | Unavailable: *"A portal frontend implementing GlobalShortcuts was not found"* (xdg-desktop-portal-cosmic lacks it). Code path (create_session, bind_shortcuts, Activated/Deactivated -> `Hotkey{pressed}`) compiles and degrades cleanly; **not exercised on GNOME 48+/KDE 6**. |
| Tray `ksni` 0.3 (StatusNotifierItem) | COSMIC session bus | Registers with `org.kde.StatusNotifierWatcher` while GPUI runs; `Activate` call and a DBusMenu `Event` (Quit) reached the GPUI pump (`tray_activations=1`, `tray_menu=["quit"]`). |

## Findings that affect the app
1. **`ashpd` default features enable `tokio`, GPUI enables `async-io`; ashpd refuses both** (`compile_error!`). In the app
   use `ashpd = { default-features = false, features = ["async-io", "global_shortcuts"] }` and `ksni` with
   `default-features = false, features = ["async-io"]`; drive them with `futures_lite::future::block_on` on a worker thread
   (zbus' async-io reactor needs no tokio). Do **not** put them on the tokio bridge.
2. COSMIC exposes the watcher name `org.kde.StatusNotifierWatcher` only once the panel applet is running (it is
   D-Bus-activatable under `com.system76.CosmicStatusNotifierWatcher`); the first ksni registration failed with
   `ServiceUnknown` until the watcher was activated. The app must retry (ksni `watcher_online`/`assume_sni_available`) and
   keep working without a tray.
3. No global hotkey exists on Wayland without the portal: on Sway/Hyprland/COSMIC and GNOME < 48 the fallback is a
   **CLI trigger**: `pando-assistant --ptt-start` / `--ptt-stop` sent to the running instance over its local socket, bound by the
   user in the compositor (Sway: `bindsym --no-repeat $mod+space exec pando-assistant --ptt-start` and
   `bindsym --release $mod+space exec pando-assistant --ptt-stop`). Document it in the user guide.
4. `global-hotkey` must be created on the main thread on macOS and needs a Win32 message loop on Windows; GPUI owns both loops,
   so create the manager inside `application().run` (as the spike does) and keep it alive for the app lifetime.
5. Tray on macOS/Windows: `tray-icon` (needs the main-thread event loop GPUI already runs; menu events are a global channel
   like `global-hotkey`) - same forwarding shape, **not built or run in this spike**.

## Pending (task stays in_review)
Hold-to-talk on macOS, Windows, KDE 6 and GNOME 48+ (portal), `tray-icon` on macOS/Windows, Sway CLI fallback end to end,
real-keyboard repeat behaviour (X11 auto-repeat can send extra Pressed events; the state machine must be idempotent).
