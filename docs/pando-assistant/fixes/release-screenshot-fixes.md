---
created_at: 2026-10-12T14:38:10.785166301Z
updated_at: 2026-10-12T14:38:10.785166301Z
tags:
    - fix
---
# Release screenshot fixes

What: six UI fixes found in release screenshots. Builds on [[pando-assistant/plans/backlog-execution_plan.md]].

1. Discovery/Detect (T-0047/0048): discovery already probed https+CA then http; stale saved URL caused 18765. firstrun/view.rs detect() now falls back to auto discovery and writes the discovered base_url into the field.
2. Split layout (T-0116): chat/render_transcript.rs item_el user bubble min_w_0 so it never overflows left.
3. First-run Voz (T-0149): firstrun/view.rs rows shrink; buttons flex_none.
4. Cell labels (T-0038/0056): cells/mapping.rs fit_label (word boundary + ellipsis) used by label_for_task, cell_for_instance.
5. Settings Voz (T-0126): voice/settings_panel.rs Switch instead of missing glyphs.
6. Paths (T-0069/0079): chat/tools.rs display_path/arg_full/arg_summary(tool, root); ChatService::load_project -> ChatMsg::Project; panel.project_root; tooltip with full path; permission dialog keeps full path below.

Verification: cargo test -p app --lib (351 pass), clippy -D warnings, check-deps, xvfb screenshots against live `pando serve` (shots/fix-*.png), cargo xtask bundle rebuilt dist/packages.
