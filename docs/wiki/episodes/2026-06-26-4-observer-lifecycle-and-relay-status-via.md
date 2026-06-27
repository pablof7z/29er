---
type: episode-card
date: 2026-06-26
session: 00700b1c-4fd7-4a41-90e9-7a76d64b42cc
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-29er/00700b1c-4fd7-4a41-90e9-7a76d64b42cc.jsonl
salience: root-cause
status: active
subjects:
  - observer-lifecycle
  - relay-connectivity
  - resource-management
  - state-inference
supersedes: []
related_claims: []
source_lines:
  - 1363-1382
captured_at: 2026-06-26T07:36:02Z
---

# Episode: Observer lifecycle and relay status via LRU eviction and heartbeat updates

## Prior State

GroupChatProjection observers never unregistered, accumulating unbounded leak over time; relay connectivity inferred from 'ever seen data' (empty channels report 'Connecting' indefinitely, disconnects stay 'Connected')

## Trigger

Codex exec review flagged dual violation: 'Retained GroupChatProjection observers leak' AND 'relay status is inferred from ever seen data so empty relays stay Connecting and disconnects stay Connected'

## Decision

Implement LRU eviction cap at 50 watched channels with automatic cleanup; switch relay status from local inference to heartbeat-driven updates via NMP kernel callbacks instead of persisting 'ever seen' state

## Consequences

- Memory bounded: observer count capped at 50, preventing unbounded accumulation as user navigates channels
- Relay state accurate: reflects actual kernel state; empty channels only report 'Connecting' while actually connecting; disconnects properly reported
- Survives reconnects: relay status not sticky, follows real kernel state dynamically
- Pattern reusable: LRU + event-driven state model applicable to other resource-constrained collections

## Open Tail

- NMP architecture violations in items 2/3/4 from codex (TUI owns projection composition vs NMP-typed snapshots, relay policy hardcoded in shell vs NMP-owned, polling snapshots vs event-driven) deferred pending architectural review

## Evidence

- transcript lines 1363-1382

## Conversation

- Cleaned transcript (verbatim user words, abbreviated agent replies): [`transcripts/2026-06-26-4-observer-lifecycle-and-relay-status-via.json`](transcripts/2026-06-26-4-observer-lifecycle-and-relay-status-via.json)
- Raw transcript (verbatim user words, full agent replies): [`transcripts/raw/2026-06-26-4-observer-lifecycle-and-relay-status-via.json`](transcripts/raw/2026-06-26-4-observer-lifecycle-and-relay-status-via.json)
