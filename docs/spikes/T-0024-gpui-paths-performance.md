# T-0024 · GPUI `paint_path` performance (spike S5)

Task: PDA-T-0024. Code: `spikes/gpui-shell/` (`gpui-shell-spikes paths|skia`, `run-paths-sweep.sh`), results in
`spikes/gpui-shell/results-paths-*.jsonl`. Date 2026-10-12, release build, gpui-kit 0.7.1.

## Scene
Synthetic organism: 98 closed/open polylines (mother membrane 120 pts, 48 daughter membranes 24 pts, 48 pseudopods 24 pts,
dashed habitat ring), every membrane/pseudopod with a wide low-alpha halo stroke. Scale 1 = **2 520 points, 195 `paint_path`
calls** (the target of the task); scale 2/4 = 2x/4x. Geometry is regenerated every frame (wobble), 24 `shape_line` labels.
Variants: `rebuild` (new `PathBuilder` + lyon tessellation each frame) vs `cached` (paths built once, cloned per frame),
labels `text-last` vs `interleaved` with paths.

## Results (CPU per frame, p50; frame interval p50)
| GPU (Vulkan, Xvfb) | scale | points / calls | build (lyon) | paint calls | interval |
|---|---|---|---|---|---|
| NVIDIA RTX 4000 Ada | 0 | 216 / 3 | 72 us | 42 us | 48 ms (floor) |
| NVIDIA | 1 | 2 520 / 195 | **0.61 ms** | **0.36 ms** | 48 ms (floor) |
| NVIDIA | 1 cached | same | 0 | 0.46 ms | 48 ms |
| NVIDIA | 1 interleaved | same | 0.61 ms | 0.35 ms | 48 ms |
| NVIDIA | 4 | 9 432 / 771 | 2.2 ms | 1.3 ms | 48 ms |
| lavapipe (llvmpipe) | 1 | 2 520 / 195 | 0.72 ms | 0.45 ms | **64 ms (15 fps)** |
| lavapipe | 1 interleaved | same | 0.74 ms | 0.43 ms | 80 ms (13 fps) |
| lavapipe | 2 | 4 824 / 387 | 1.36 ms | 0.77 ms | 80 ms |
| tiny-skia, 1000x1000 CPU raster (single thread) | 1 | 2 520 | - | 8.6 ms | - |
| tiny-skia | 2 / 4 | 4.8k / 9.4k | - | 15.9 / 30.3 ms | - |

## Findings
1. **Direct GPUI rendering is cheap on a real GPU**: tessellating + painting the target scene costs ~1 ms of CPU per frame
   (4x scene: 3.5 ms). Caching paths saves the 0.6 ms build but adds a clone cost, so caching is **not** needed at the
   target scale; the organism can rebuild its paths every frame (it animates every vertex anyway).
2. Order "all paths, then text" vs interleaved: no measurable CPU difference on NVIDIA; on lavapipe interleaving was one
   step slower (80 vs 64 ms). Keep paths first, text last (also the plan in `docs/research/organism-port.md`).
3. The frame interval under Xvfb is quantised to a 48 ms floor (X11 present path over Xvfb), so **GPU time is not
   observable here**; the NVIDIA runs only show that even 4x the scene does not push past that floor. Frame time on
   the real compositor path still has to be measured.
4. Software Vulkan (lavapipe) is the realistic worst case: 13-15 fps at the target scene. A CPU raster of the same scene
   with tiny-skia costs 8.6 ms at 1000x1000 (116 fps before the blit/upload), i.e. faster than lavapipe.
5. GPUI renders an `img()` SVG at 2x its intrinsic size and warns past 8192 px (found while integrating the wordmark, T-0007).

## Decision
- **Primary: direct GPUI paths** (`paint_path`, one canvas, paths first then text). Matches ADR-0006.
- **tiny-skia is a runtime fallback only when GPUI reports `gpu_specs().is_software_emulated`** (llvmpipe / no GPU):
  the organism renders into a pixmap at reduced size (<= 600 px) and 30 fps, then uploads it as an image. Everything else
  (non-software GPUs) never loads it. At the target scene this keeps the organism above 30 fps where GPUI-on-lavapipe is at
  13-15 fps. The rest of the GPUI UI is still slow on software Vulkan; this only protects the organism.

## Not covered / pending (task stays in_review)
- Intel/AMD integrated GPUs and Apple Silicon: only an Intel + NVIDIA box was available, and the Wayland session of the dev
  machine delivers no frames to a background window, so no on-compositor frame times; Xvfb cannot select the Intel adapter
  (`ZED_DEVICE_ID` ignored, the compositor-GPU hint wins). Re-run `run-paths-sweep.sh` on a focused real session on each GPU
  and read `interval_us` (vsync-locked 16.7 ms means headroom, anything above means saturation).
- MSAA pass count is internal to `gpui_wgpu`; not instrumented.
