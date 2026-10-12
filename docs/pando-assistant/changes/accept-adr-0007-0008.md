---
created_at: 2026-10-12T07:23:56.546651183Z
updated_at: 2026-10-12T07:23:56.546651183Z
---
# Accept ADR 0007 and 0008; update licence notes

**What changed**
- ADR 0007 (espeak-ng GPL-3 isolated in `pando-tts`; permissive `pando-stt`) and ADR 0008 (Whistle default STT, Parakeet optional) set to accepted, dated 2026-10-12.
- ADR 0004 marked "accepted; partially superseded by 0007".
- README § License rewritten for the two voice workers.
- AGENTS.md §5 layout: `voice-worker/` replaced by `pando-stt/` and `pando-tts/`; §6 licensing bullet now refers to `pando-tts`.

**Files touched**
- docs/adr/0007-espeak-ng-gpl-isolated-tts-worker.md, docs/adr/0008-stt-engines-whistle-default-parakeet-optional.md, docs/adr/0004-voice-worker-separate-process.md, docs/adr/README.md
- README.md, AGENTS.md
- gintrack: PDA-T-0023 (comment, status done), PDA-T-0017 (comment: design defaults approved)

**Why**
Project owner approved ADR 0007 and the T-0017 design defaults on 2026-10-12; ADR 0008 accepted by the director. Refs: PDA-T-0023, PDA-T-0018, PDA-T-0017.

**Verification**
Docs-only change. Each exact-text replacement was asserted to match exactly once. `git diff --stat` reviewed. No code or build affected.

Builds on [[pando-assistant/adr/0007-espeak-ng-gpl-isolated-tts-worker.md]] and [[pando-assistant/adr/0004-voice-worker-separate-process.md]].
