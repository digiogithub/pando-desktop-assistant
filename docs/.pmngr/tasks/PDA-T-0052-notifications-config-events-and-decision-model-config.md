---
id: PDA-T-0052
type: task
title: Notifications, config events and decision-model config
status: done
parent: PDA-US-0013
milestone: PDA-M-0003
assignees: [claude-sonnet]
author: mcp
labels: [pando-client]
estimate: 2
created: 2026-10-11T21:50:04Z
updated: 2026-10-11T22:37:58Z
started: 2026-10-11T22:37:50Z
closed: 2026-10-11T22:37:58Z
---

## Description
Streams `/notifications/stream` (`{id,time,level,source,message,ttl}`) and `/config/events`; `GET /config/decision-model` and `/decision-model/router/health`.

## Acceptance Criteria
- Decision-model change in Pando ⇒ event received and config reloaded.
