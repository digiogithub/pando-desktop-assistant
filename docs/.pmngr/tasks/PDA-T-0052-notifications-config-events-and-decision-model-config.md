---
id: PDA-T-0052
type: task
title: Notifications, config events and decision-model config
status: backlog
parent: PDA-US-0013
milestone: PDA-M-0003
author: mcp
labels: [pando-client]
estimate: 2
created: 2026-10-11T21:50:04Z
updated: 2026-10-11T22:03:06Z
---

## Description
Streams `/notifications/stream` (`{id,time,level,source,message,ttl}`) and `/config/events`; `GET /config/decision-model` and `/decision-model/router/health`.

## Acceptance Criteria
- Decision-model change in Pando ⇒ event received and config reloaded.
