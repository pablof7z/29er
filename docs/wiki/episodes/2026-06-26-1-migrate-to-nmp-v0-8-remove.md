---
type: episode-card
date: 2026-06-26
session: 00700b1c-4fd7-4a41-90e9-7a76d64b42cc
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-29er/00700b1c-4fd7-4a41-90e9-7a76d64b42cc.jsonl
salience: architecture
status: active
subjects:
  - nmp-integration
  - message-composition
  - dependency-management
  - mention-bug-fix
supersedes: []
related_claims: []
source_lines:
  - 2018-2030
  - 2080-2140
  - 2605-2625
captured_at: 2026-06-26T13:01:44Z
---

# Episode: Migrate to NMP v0.8, remove vendored submodule, centralize message composition to shared Rust core

## Prior State

NMP consumed as vendored git submodule (vendor/nmp). Message composition (@mention rewriting, NIP-21 notation) implemented separately in shell layers (TUI, Swift) — source of the mention bug that had to be fixed in two places. Entrypoint action was `post_chat_message` (NMP v0.7.2).

## Trigger

NMP #2131 merged to master, introducing `publish_group_event` as generic group-message action, retiring `post_chat_message`. Compile errors (13 across nmp-app-29er and shakeout) due to v0.7.2→v0.8.0 breaking API changes (register/projection submodule restructuring, NmpApp method deletions).

## Decision

Remove vendor/nmp submodule entirely; consume NMP as git dependency pinned to master (1f798ef). Centralize message composition in nmp-app-29er::compose_chat_message() — the single source of truth for @mention→nostr:npub1 rewriting and p-tag generation. Retire shell-level composer code. Route all shells (TUI, Swift) through nmp.nip29.publish_group_event. Close PR #14 (shell-level hack, now obsolete).

## Consequences

- Mention bug fixed once in shared Rust; both TUI and iOS inherit the fix automatically
- TUI and iOS no longer compose locally — they hand raw text + picked pubkeys to app core only
- Cleaner layering: envelope + signing stay in NMP; composition in app; shells delegate
- Git rev pinning simpler than submodule management; no vendoring overhead
- Orphaned code surfaced and deleted: group_chat_history_interest / group_chat_live_interest (v0.7.2 subscription builders, superseded by v0.8.0's open_group_chat all-in-one door)
- Migration verified: cargo check clean, 66 tests passing, iOS xcodebuild BUILD SUCCEEDED

## Open Tail

*(none)*

## Evidence

- transcript lines 2018-2030
- transcript lines 2080-2140
- transcript lines 2605-2625

## Conversation

- Cleaned transcript (verbatim user words, abbreviated agent replies): [`transcripts/2026-06-26-1-migrate-to-nmp-v0-8-remove.json`](transcripts/2026-06-26-1-migrate-to-nmp-v0-8-remove.json)
- Raw transcript (verbatim user words, full agent replies): [`transcripts/raw/2026-06-26-1-migrate-to-nmp-v0-8-remove.json`](transcripts/raw/2026-06-26-1-migrate-to-nmp-v0-8-remove.json)
