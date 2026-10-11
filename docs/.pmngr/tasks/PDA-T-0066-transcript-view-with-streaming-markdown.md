---
id: PDA-T-0066
type: task
title: Transcript view with streaming markdown
status: backlog
parent: PDA-US-0018
milestone: PDA-M-0004
author: mcp
labels: [chat]
estimate: 5
created: 2026-10-11T21:51:26Z
updated: 2026-10-11T22:02:19Z
---

## Description
`MessageScroller` (FollowMode::Tail + "ir al final" pill), grouped user/assistant bubbles, `TextViewState::markdown` + `push_str` for deltas, code blocks with highlighting (tree-sitter features of kit) and a copy button, unclosed fence rendered as code, layouts cached per message.

## Acceptance Criteria
- ≥ 50 fps with 2,000 deltas/3 s; manual scroll stops the follow.
