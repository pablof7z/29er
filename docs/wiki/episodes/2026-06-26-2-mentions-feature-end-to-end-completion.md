---
type: episode-card
date: 2026-06-26
session: 00700b1c-4fd7-4a41-90e9-7a76d64b42cc
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-29er/00700b1c-4fd7-4a41-90e9-7a76d64b42cc.jsonl
salience: product
status: active
subjects:
  - mentions-feature
  - composer
  - nmp-dispatch
supersedes: []
related_claims: []
source_lines:
  - 1363-1382
captured_at: 2026-06-26T07:36:02Z
---

# Episode: Mentions feature end-to-end completion with pubkey dispatch

## Prior State

Composer displayed @mention popup from GroupMembersSnapshot and rendered mentions in text, but selected mention pubkeys were never captured or passed to NMP action; feature was half-implemented

## Trigger

Codex exec review detected 'selected mention pubkeys are never dispatched' during UX improvements verification; composer.rs inserts text but app.rs sends only content to NMP

## Decision

Implement pubkey tracking in ComposerComponent; on send, collect selected pubkey vector and pass to nmp_nip29 action dispatch; fix mention tier detection to match on pubkey instead of text pattern

## Consequences

- Mentions feature works end-to-end: users can @mention and dispatch includes mention metadata
- Reply tiers properly detect received mentions via pubkey matching (not text pattern)
- Unread/notification system can distinguish mention messages from regular messages

## Open Tail

*(none)*

## Evidence

- transcript lines 1363-1382

## Conversation

- Cleaned transcript (verbatim user words, abbreviated agent replies): [`transcripts/2026-06-26-2-mentions-feature-end-to-end-completion.json`](transcripts/2026-06-26-2-mentions-feature-end-to-end-completion.json)
- Raw transcript (verbatim user words, full agent replies): [`transcripts/raw/2026-06-26-2-mentions-feature-end-to-end-completion.json`](transcripts/raw/2026-06-26-2-mentions-feature-end-to-end-completion.json)
