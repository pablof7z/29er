---
type: episode-card
date: 2026-06-26
session: 00700b1c-4fd7-4a41-90e9-7a76d64b42cc
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-29er/00700b1c-4fd7-4a41-90e9-7a76d64b42cc.jsonl
salience: root-cause
status: active
subjects:
  - outbox-state
  - message-confirmation
  - data-matching
supersedes: []
related_claims: []
source_lines:
  - 1363-1382
captured_at: 2026-06-26T07:36:02Z
---

# Episode: Outbox reliability via (id, timestamp, pubkey) tuple matching

## Prior State

Outbox items matched by message content only; when my_pubkey is missing from state, any message with identical text could be confirmed; duplicate sends of same message cause cross-confirmation race

## Trigger

Codex exec review found race condition: 'Outbox confirmation matches by message content and accepts any pubkey when my_pubkey is missing, so it can confirm the wrong item or never confirm'

## Decision

Match outbox items by (message_id, timestamp, pubkey) tuple instead of content only; add guard requiring my_pubkey presence for match; add 60s timeout for automatic cleanup of unconfirmed items

## Consequences

- Outbox confirmation is precise: immutable triple matches intended message exactly
- Eliminates false positives when user sends duplicate text
- Graceful timeout prevents stale pending messages from accumulating
- Pattern reusable for other state-tracking problems requiring composite identity keys

## Open Tail

*(none)*

## Evidence

- transcript lines 1363-1382

## Conversation

- Cleaned transcript (verbatim user words, abbreviated agent replies): [`transcripts/2026-06-26-3-outbox-reliability-via-id-timestamp-pubkey.json`](transcripts/2026-06-26-3-outbox-reliability-via-id-timestamp-pubkey.json)
- Raw transcript (verbatim user words, full agent replies): [`transcripts/raw/2026-06-26-3-outbox-reliability-via-id-timestamp-pubkey.json`](transcripts/raw/2026-06-26-3-outbox-reliability-via-id-timestamp-pubkey.json)
