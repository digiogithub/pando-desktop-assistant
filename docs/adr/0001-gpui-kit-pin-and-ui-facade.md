# 0001 · GPUI only through `gpui-kit =0.7.1`, behind a `ui/` facade

- **Status:** accepted
- **Date:** 2026-10-12
- **Deciders:** project owner, planning agents
- **Refs:** PDA-T-0005, PDA-T-0014, [`docs/research/bitacora-gpui.md`](../research/bitacora-gpui.md) §0, §1.2–1.3, §5.1

## Context

GPUI is not published as a stable crate. Bitácora (our reference GPUI app) depends on
`gpui-kit = "=0.7.1"` (Longbridge, Apache-2.0), which pins the weekly `gpui-pre =0.3.8`
snapshot of Zed. Kit releases have renamed APIs within a month (0.6 renamed everything, 0.7
changed the window entry point). APIs in older Zed docs (`Model<T>`, `ViewContext`,
`cx.request_animation_frame`) are stale. Zed's own UI crates (`editor`, `ui`, `theme`) are
GPL and must not be copied.

## Decision

We depend on GPUI only through `gpui-kit = "=0.7.1"` (exact pin), and only in the `app`
crate. Inside `app`, only `crates/app/src/ui/` may name `gpui_kit::`; the rest of the app
imports the facade. A test enforces the rule, `cargo xtask check-deps` enforces that no
other crate depends on `gpui-kit`, and a canary workflow builds against the next kit
release.

## Consequences

- Upgrades are deliberate, monthly at most, and touch one module.
- `organism`, `decide` and `pando-client` stay GPUI-free and testable without a GPU.
- We use current GPUI idioms: `Entity<T>`, `window.request_animation_frame()`.
- Features missing in GPUI (tray, global hotkey, always-on-top on Wayland) come from
  external crates outside the facade.

## Alternatives considered

| Option | Why not |
|---|---|
| `gpui 0.2.2` from crates.io | Older than the snapshot gpui-kit uses; no component library |
| Zed git dependency | Moves daily, heavier builds, no Bitácora experience to reuse |
| Naming `gpui_kit` everywhere | Every kit rename becomes a repo-wide refactor |
