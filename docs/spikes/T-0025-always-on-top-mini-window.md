# T-0025 · Always-on-top transparent mini window (spike S8)

Task: PDA-T-0025. Code: `spikes/gpui-shell/src/mini.rs` (`gpui-shell-spikes mini --kind normal|popup|floating|layer [--opaque]`).
GPUI source read: `gpui-pre-0.3.8` and `gpui-pre-linux-0.3.8` (`x11/window.rs`, `wayland/window.rs`).

## What GPUI does per kind
| `WindowKind` | X11 | Wayland | macOS / Windows (source/docs, not run) |
|---|---|---|---|
| `Normal` | managed toplevel | xdg_toplevel | normal window |
| `PopUp` | **override-redirect + `_NET_WM_WINDOW_TYPE_NOTIFICATION`** (verified): above everything, no WM, no decorations | **plain xdg_toplevel**, no always-on-top (Wayland has none) | topmost / floating panel level |
| `Floating` | managed toplevel, **no `_NET_WM_STATE_ABOVE`**, no transient (verified: no atoms set) | xdg_toplevel with `set_parent` (above its parent only, needs a parent window) | child above parent |
| `LayerShell(Overlay)` | n/a | `zwlr_layer_shell_v1` overlay layer: real always-on-top, anchored, keyboard modes None/OnDemand/Exclusive | n/a |
| `Dialog` | transient + modal | xdg_dialog | modal |
Transparency: `WindowBackgroundAppearance::Transparent` creates a depth-32 ARGB visual on X11 (verified, `Depth: 32`);
it only shows through when a compositing manager runs.

## Measured here (Xvfb, no WM/compositor; live COSMIC Wayland session)
- X11/Xvfb: Normal, PopUp and Floating all open a 360x120 frameless ARGB window, render continuously (~41 fps loop),
  receive mouse presses (`start_window_move` handler fired) and keyboard input when the pointer is over them (3/3 keys), including the
  override-redirect PopUp (it gets keys through XInput after pointer focus; with a real WM an OR window is not given
  focus by the WM, so a click-to-focus path is needed).
- COSMIC Wayland (cosmic-comp 1.0): `LayerShell` window **opened without `LayerShellNotSupportedError`**, i.e. wlr-layer-shell
  is available on COSMIC. `GlobalShortcuts` portal is not (see T-0026).
- The Wayland session of the dev box does not deliver frame callbacks to windows of a background agent process (3 frames in 5 s
  for every kind), so visibility, stacking, transparency and focus on Wayland were **not observable** here.

## Gaps in GPUI worth knowing
- No setter for layer-shell margin/anchor after creation (only `set_exclusive_zone/edge`), and `start_window_move` is a no-op
  for layer surfaces (it needs an xdg_toplevel). A draggable mini window on Wayland therefore cannot be a layer surface unless
  we re-create it with new margins on drag end, or patch GPUI.
- GNOME/Mutter does not implement wlr-layer-shell: `open_window` fails with `LayerShellNotSupportedError` there.

## Choice per platform (proposal)
| Platform | Mini window |
|---|---|
| X11 (any WM) | `PopUp` (override-redirect, notification type) or `Normal` + `_NET_WM_STATE_ABOVE` set via a small X11 call; custom drag with `start_window_move` |
| Wayland wlroots/KDE/COSMIC | `LayerShell` Overlay, anchored top-right, `KeyboardInteractivity::OnDemand`; reposition = recreate (until GPUI exposes margins) |
| Wayland GNOME | `Normal` frameless transparent window; "always on top" is not available to apps: document the window-menu toggle / optional shell extension; fall back by catching `LayerShellNotSupportedError` |
| macOS / Windows | `PopUp` (topmost) + `Transparent`; custom drag via `start_window_move` / `WindowControlArea::Drag` |

## Pending (task stays in_review)
Run `mini` on a focused real session of: macOS, Windows, GNOME Wayland (expect the layer-shell error), KDE Wayland, Sway,
COSMIC; confirm stacking over fullscreen apps, transparency with the compositor, and keyboard focus (`OnDemand` vs
`Exclusive`). Extend `mini.rs` with a drag-by-recreate test for layer surfaces.
