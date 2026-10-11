---
id: PDA-M-0010
type: milestone
title: "M9 · Hands-free: voice invocation"
status: backlog
author: mcp
created: 2026-10-11T21:40:56Z
updated: 2026-10-11T22:02:15Z
due: 2027-04-09
---

## Description
Invoke the orb by voice ("Oye Pando"): continuous capture → Silero VAD → short Whistle pass with a "Pando" bias → fuzzy match. AEC with `sonora` for barge-in without headphones.

## Acceptance Criteria
- FAR < 1/hour over 3 h of office audio; FRR < 10% at 2 m.
- Idle CPU < 3% of one core with continuous listening.
- Hands-free barge-in with no false triggers over 5 min of speaker TTS (with AEC).

## Notes
Refs: `docs/research/voice-stack.md` §2, §4.5.
