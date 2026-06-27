# Wiki Index

> Derived cache — do not hand-edit. Rebuilt by proactive-context after each capture.

Last updated: 2026-06-27

## Research Records (2 records)

| Record | Date | Finding | Agent |
|--------|------|---------|-------|
| [2026-06-26-1-audit-of-29er-ios-swift-against](research/2026-06-26-1-audit-of-29er-ios-swift-against.md) | 2026-06-26 | Audit of 29er iOS Swift against NMP doctrine: 18 raw findings screened via adversarial verification to 8 confirmed violations (HIGH/MEDIUM severity), each with concrete fixes and recommended remediation order | main |
| [AGENTS](research/AGENTS.md) |  |  |  |

## Episode Cards (3 cards)

| Card | Date | Title | Salience | Status |
|------|------|-------|----------|--------|
| [2026-06-26-1-migrate-to-nmp-v0-8-remove](episodes/2026-06-26-1-migrate-to-nmp-v0-8-remove.md) | 2026-06-26 | Migrate to NMP v0.8, remove vendored submodule, centralize message composition to shared Rust core | architecture | active |
| [2026-06-26-2-enforce-nmp-doctrine-eliminate-protocol-logic](episodes/2026-06-26-2-enforce-nmp-doctrine-eliminate-protocol-logic.md) | 2026-06-26 | Enforce NMP doctrine: eliminate protocol logic from shells and app layer, establish clear boundary (shell=renderer, app=router+composer, NMP=protocol owner) | architecture | active |
| [2026-06-27-1-chat-ui-implementation-adopt-nmp-content](episodes/2026-06-27-1-chat-ui-implementation-adopt-nmp-content.md) | 2026-06-27 | Chat UI implementation — adopt NMP content renderer and registry components | architecture | active |

## Nouns (19 entities)

| Noun | Name | Origin | Definition |
|------|------|--------|------------|
| [29er](nouns/29er.md) | 29er | extracted | Nostr group chat TUI using Ratatui (0.30+), running on Rust 2021, consuming NMP v0.8 as a git dependency, with iOS sibling shell sharing nmp-app-29er core composition |
| [compose-chat-message](nouns/compose-chat-message.md) | compose_chat_message | extracted | shared Rust function in nmp-app-29er that transforms raw text + mention pubkeys into kind:9 content with NIP-21 (nostr:npub1...) formatting + p-tags, called by both TUI and iOS before dispatch |
| [contenttreewire](nouns/contenttreewire.md) | ContentTreeWire | extracted | serde-serializable FFI wire projection of ContentTree; flat index arena with u32 indices instead of recursive references |
| [groupchatmessage](nouns/groupchatmessage.md) | GroupChatMessage | extracted | flat carrier where threading/reply nesting is deliberately not modeled; minimum fields needed for shell to draw a row |
| [grouptreeprojection](nouns/grouptreeprojection.md) | GroupTreeProjection | extracted | folds kind:9 into per-group last-message preview + recursive unread counts, source of truth that must be reused not reimplemented |
| [nmp](nouns/nmp.md) | NMP | extracted | Rust event-processing kernel for Nostr event processing with hard separation: business logic (kinds, tags, signing, relay routing, NIP-29, unread aggregation) lives in Rust; shells only render |
| [nmp-app-29er](nouns/nmp-app-29er.md) | nmp-app-29er | extracted | the 29er-specific glue crate in 29er repo (not vendored), re-exports whole 29er surface, contains GroupTreeProjection for unread + last-message preview, exposes shared composition (post_chat_message) via FFI |
| [nmp-content](nouns/nmp-content.md) | nmp-content | extracted | Layer A substrate producing ContentTree/Segment IR with FlatBuffers wire format for shells |
| [nmp-content-tokenize-text](nouns/nmp-content-tokenize-text.md) | nmp_content_tokenize_text | extracted | pure content-tokenizer C-ABI; FFI wrapper around nmp-content's tokenizer; does not resolve entities |
| [nmp-doctrine](nouns/nmp-doctrine.md) | NMP doctrine | extracted | kernel emits, per-app crate composes, shell only renders |
| [nmp-nip29](nouns/nmp-nip29.md) | nmp-nip29 | extracted | NIP-29 relay groups (chat rooms/channels), the core domain crate |
| [nostrcontentview](nouns/nostrcontentview.md) | NostrContentView | extracted | SwiftUI renderer for ContentTreeWire that walks tree.roots and flattens the arena into block-level groups with inline text concatenation |
| [publish-group-event](nouns/publish-group-event.md) | publish_group_event | extracted | the canonical NMP action surface (nmp.nip29.publish_group_event): app says 'publish (kind, content, tags) to group X', NMP injects envelope (h, previous, routing) and signs |
| [rich-content-rendering](nouns/rich-content-rendering.md) | rich content rendering | extracted | replace raw-content Text with NMP content renderer so mentions/embeds/media/hashtags/markdown render properly |
| [segment-ir](nouns/segment-ir.md) | Segment IR | extracted | internal, ergonomic, recursive representation that is NOT serde-serializable; projects to ContentTreeWire wire form |
| [slack-layout](nouns/slack-layout.md) | Slack layout | extracted | pure presentation layer: group consecutive messages by author with one header per run, day dividers, avatar/name gutter, aligned timestamps |
| [swift-app](nouns/swift-app.md) | Swift app | extracted | pure renderer over the C-ABI + FlatBuffers boundary |
| [tokenizer](nouns/tokenizer.md) | tokenizer | extracted | render substrate (Layer A) |
| [tui](nouns/tui.md) | TUI | extracted | pure renderer + input layer over NMP snapshots, mirroring the Swift shell |

