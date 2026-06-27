---
type: episode-card
date: 2026-06-26
session: 00700b1c-4fd7-4a41-90e9-7a76d64b42cc
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-29er/00700b1c-4fd7-4a41-90e9-7a76d64b42cc.jsonl
salience: architecture
status: active
subjects:
  - nmp-doctrine
  - architecture-boundaries
  - group-membership-truth
  - protocol-ownership
supersedes: []
related_claims: []
source_lines:
  - 2659-2727
  - 2762-2814
  - 2833-2852
  - 2855-2898
captured_at: 2026-06-26T13:01:44Z
---

# Episode: Enforce NMP doctrine: eliminate protocol logic from shells and app layer, establish clear boundary (shell=renderer, app=router+composer, NMP=protocol owner)

## Prior State

Swift shell and Rust app layer (nmp-app-29er) independently implement or derive protocol-level logic: outbox/chat join + dedup via raw ["h", groupId] tag walking and magic kind:9 filtering; membership/admin truth computed by roster scan; @mention tokenization + NIP-19 bech32 detection; relay defaults hardcoded in Swift; group membership gating via locally-computed flags. Rust layer hand-registers KernelEvent taps and raw global {"kinds":[9]} interests instead of using NMP's abstraction layer.

## Trigger

Adversarial audit (comparing 29er Swift against NMP API surface + Chirp reference shell) confirmed 8 doctrine violations. Round-2 codex deep-pass found 2 additional root issues: (a) Rust nmp-app-29er doing kernel-level work (raw event taps + raw interests) instead of calling NMP hydrating doors; (b) Join button permanently disabled due to gating on dead group_members no-op projection instead of actual JoinedGroupsProjection membership truth.

## Decision

10-fix refactor across two rounds: (1–2) Emit optimistic messages with delivery status in Rust group_chat projection; remove non-canonical kind+tags fields from PublishOutboxItem. (3–4) Wire is_member/is_admin from JoinedGroupsProjection onto GroupTreeNode; gate Join button and admin UI on projection truth. (5) Delete @mention tokenizers entirely; shells send only raw text + picked pubkeys. (6–8) Move relay defaults to Rust nmp-app-29er config, expose seed_default_relays FFI, use wire_group_defaults_with_relay; strip all Swift relay URL literals. Plus round-2: refactor nmp-app-29er to use NMP hydrating doors (open_group_discovery / open_joined_groups / open_group_chat) instead of hand-tapping raw events; preserve GroupTreeProjection unread/preview composition as legitimate app-layer work.

## Consequences

- Swift shell becomes pure renderer: no event parsing, tag walking, kind filtering, content tokenization, membership derivation, or relay selection logic
- Single source of truth for group membership (JoinedGroupsProjection.is_member/is_admin), chat delivery status, and relay defaults — all kernel-projected and read by app/shell
- Join button unblocked; was permanently false due to scanning dead roster path
- Admin UI and composer now gate correctly on actual projected membership/admin state rather than computed ghosts
- Rust app layer delegates subscription + projection management to NMP doors instead of kernel-level hand-crafting
- Doctrine boundaries now enforced: NMP owns events/kinds/tags/envelope/signing; app owns composition+routing+display logic; shell owns rendering only
- Both shells green: cargo check clean, xcframework rebuilds, iOS xcodebuild BUILD SUCCEEDED; workspace exit 0

## Open Tail

*(none)*

## Evidence

- transcript lines 2659-2727
- transcript lines 2762-2814
- transcript lines 2833-2852
- transcript lines 2855-2898

## Conversation

- Cleaned transcript (verbatim user words, abbreviated agent replies): [`transcripts/2026-06-26-2-enforce-nmp-doctrine-eliminate-protocol-logic.json`](transcripts/2026-06-26-2-enforce-nmp-doctrine-eliminate-protocol-logic.json)
- Raw transcript (verbatim user words, full agent replies): [`transcripts/raw/2026-06-26-2-enforce-nmp-doctrine-eliminate-protocol-logic.json`](transcripts/raw/2026-06-26-2-enforce-nmp-doctrine-eliminate-protocol-logic.json)
