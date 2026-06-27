# Wiki Index

> Derived cache — do not hand-edit. Rebuilt by proactive-context after each capture.

Last updated: 2026-06-27

## git-configuration (1 guide)

| Slug | Title | Summary | Tags | Volatility | Verified | Topic |
|------|-------|---------|------|------------|----------|-------|
| [claude-directory-gitignore](guides/claude-directory-gitignore.md) | Excluding .claude/ from Version Control | The `.claude/` directory contains local Claude-related state and configuration and should not be committed to version control. | capture | warm | 2026-06-27 | git-configuration |

## group-management (1 guide)

| Slug | Title | Summary | Tags | Volatility | Verified | Topic |
|------|-------|---------|------|------------|----------|-------|
| [group-management](guides/group-management.md) | Group Management | Join, leave, and admin action forms consume dispatch error results before displaying success/failure status. | capture | warm | 2026-06-26 | group-management |

## mention-selection (1 guide)

| Slug | Title | Summary | Tags | Volatility | Verified | Topic |
|------|-------|---------|------|------------|----------|-------|
| [mention-selection](guides/mention-selection.md) | Mention Selection | Mention selections dispatch pubkeys as typed identifiers | capture | warm | 2026-06-26 | mention-selection |

## message-confirmation (1 guide)

| Slug | Title | Summary | Tags | Volatility | Verified | Topic |
|------|-------|---------|------|------------|----------|-------|
| [message-confirmation](guides/message-confirmation.md) | Message Sending and Confirmation | Outbox confirmation uses NMP action-stage correlation state to uniquely identify sent messages and verify sender identity. | capture | warm | 2026-06-26 | message-confirmation |

## relay-selection (1 guide)

| Slug | Title | Summary | Tags | Volatility | Verified | Topic |
|------|-------|---------|------|------------|----------|-------|
| [relay-selection](guides/relay-selection.md) | Relay Selection and Status | Relay selection, default relay, and fallback policy are owned by Rust/NMP state and actions; the TUI renders relay state and dispatches selection intent. | capture | warm | 2026-06-26 | relay-selection |

## snapshot-system (1 guide)

| Slug | Title | Summary | Tags | Volatility | Verified | Topic |
|------|-------|---------|------|------------|----------|-------|
| [snapshot-system](guides/snapshot-system.md) | TUI Snapshots and State Management | The 29er app sources group discovery, membership, and chat state via NMP's open_group_discovery, open_joined_groups, and open_group_chat APIs | capture | warm | 2026-06-26 | snapshot-system |

## Research Records (12 records)

| Record | Date | Finding | Agent |
|--------|------|---------|-------|
| [2026-06-26-1-audit-of-29er-ios-swift-against](research/2026-06-26-1-audit-of-29er-ios-swift-against.md) | 2026-06-26 | Audit of 29er iOS Swift against NMP doctrine: 18 raw findings screened via adversarial verification to 8 confirmed violations (HIGH/MEDIUM severity), each with concrete fixes and recommended remediation order | main |
| [2026-06-26-1-codex-exec-code-review-gate-verdict](research/2026-06-26-1-codex-exec-code-review-gate-verdict.md) | 2026-06-26 | Codex exec code review gate; verdict needs-fixes identifying 7 blocking issues (test compilation, architecture violations, mentions feature, outbox confirmation, observer leaks) | main |
| [2026-06-26-1-doctrine-audit-of-29er-ios-swift](research/2026-06-26-1-doctrine-audit-of-29er-ios-swift.md) | 2026-06-26 | Doctrine audit of 29er iOS Swift: seven parallel finders across predefined dimensions; 18 raw findings filtered to 8 confirmed violations via adversarial verification against NMP API and Chirp reference | main |
| [2026-06-26-1-nmp-v0-8-0-migration-verification](research/2026-06-26-1-nmp-v0-8-0-migration-verification.md) | 2026-06-26 | NMP v0.8.0 migration verification: workspace clean, 66 tests pass, vendoring eliminated | main |
| [2026-06-26-1-opus-code-review-of-scaffolding-pr](research/2026-06-26-1-opus-code-review-of-scaffolding-pr.md) | 2026-06-26 | Opus code review of scaffolding PR #12: 5 issues found (workspace check, dead deps, naming mismatch, unsafe blocks, polish gaps), verdict NEEDS_WORK | main |
| [2026-06-26-1-swift-codebase-audit-for-nmp-doctrine](research/2026-06-26-1-swift-codebase-audit-for-nmp-doctrine.md) | 2026-06-26 | Swift codebase audit for NMP doctrine violations: 18 findings verified by 7 parallel finders, 8 confirmed violations (HIGH/MEDIUM severity) with prioritized fix order | main |
| [2026-06-26-2-implementation-code-review-verdict-opus-approves](research/2026-06-26-2-implementation-code-review-verdict-opus-approves.md) | 2026-06-26 | Implementation code review verdict: Opus approves 8 doctrine fixes, Codex blocks on 2 additional violations (Rust raw-tap doctrine violation, Join-button regression) | main |
| [2026-06-26-2-nmp-doctrine-compliance-audit-five-architectural](research/2026-06-26-2-nmp-doctrine-compliance-audit-five-architectural.md) | 2026-06-26 | NMP doctrine compliance audit: five architectural checks (TUI/composer/publish_group_event/vendoring/tests) verified as passing | main |
| [2026-06-26-2-opus-review-of-8-issue-epic](research/2026-06-26-2-opus-review-of-8-issue-epic.md) | 2026-06-26 | Opus review of 8-issue epic (#3-#10): all acceptance criteria passed, 1,862 lines of TUI code, 37 agents, verdict APPROVE | main |
| [2026-06-26-3-codex-exec-evaluation-of-ux-ui](research/2026-06-26-3-codex-exec-evaluation-of-ux-ui.md) | 2026-06-26 | Codex exec evaluation of UX/UI improvements workflow: 7 blocking issues identified (test compilation, mentions dispatch, outbox confirmation, observer leak, NMP architecture violations, relay state), verdict needs-fixes | main |
| [2026-06-27-1-audit-of-git-stashes-and-worktrees](research/2026-06-27-1-audit-of-git-stashes-and-worktrees.md) | 2026-06-27 | Audit of git stashes and worktrees to identify features missing from main; verdict to restore relay browser (stash@{0}), cherry-pick TUI popup membership fix, discard superseded work | afeeb8bbacdf85127 |
| [AGENTS](research/AGENTS.md) |  |  |  |

## Episode Cards (8 cards)

| Card | Date | Title | Salience | Status |
|------|------|-------|----------|--------|
| [2026-06-26-1-migrate-to-nmp-v0-8-remove](episodes/2026-06-26-1-migrate-to-nmp-v0-8-remove.md) | 2026-06-26 | Migrate to NMP v0.8, remove vendored submodule, centralize message composition to shared Rust core | architecture | active |
| [2026-06-26-1-ratatui-0-29-0-30-upgrade](episodes/2026-06-26-1-ratatui-0-29-0-30-upgrade.md) | 2026-06-26 | Ratatui 0.29 → 0.30 upgrade for modern widget ecosystem | architecture | active |
| [2026-06-26-2-enforce-nmp-doctrine-eliminate-protocol-logic](episodes/2026-06-26-2-enforce-nmp-doctrine-eliminate-protocol-logic.md) | 2026-06-26 | Enforce NMP doctrine: eliminate protocol logic from shells and app layer, establish clear boundary (shell=renderer, app=router+composer, NMP=protocol owner) | architecture | active |
| [2026-06-26-2-mentions-feature-end-to-end-completion](episodes/2026-06-26-2-mentions-feature-end-to-end-completion.md) | 2026-06-26 | Mentions feature end-to-end completion with pubkey dispatch | product | active |
| [2026-06-26-3-outbox-reliability-via-id-timestamp-pubkey](episodes/2026-06-26-3-outbox-reliability-via-id-timestamp-pubkey.md) | 2026-06-26 | Outbox reliability via (id, timestamp, pubkey) tuple matching | root-cause | active |
| [2026-06-26-4-observer-lifecycle-and-relay-status-via](episodes/2026-06-26-4-observer-lifecycle-and-relay-status-via.md) | 2026-06-26 | Observer lifecycle and relay status via LRU eviction and heartbeat updates | root-cause | active |
| [2026-06-27-1-chat-ui-implementation-adopt-nmp-content](episodes/2026-06-27-1-chat-ui-implementation-adopt-nmp-content.md) | 2026-06-27 | Chat UI implementation — adopt NMP content renderer and registry components | architecture | active |
| [2026-06-27-1-upgrade-to-nmp-v0-8-4](episodes/2026-06-27-1-upgrade-to-nmp-v0-8-4.md) | 2026-06-27 | Upgrade to NMP v0.8.4 event registration API (kind-specific groups, open_group_discovery) | architecture | active |

## Nouns (78 entities)

| Noun | Name | Origin | Definition |
|------|------|--------|------------|
| [29er](nouns/29er.md) | 29er | extracted | Nostr group chat TUI using Ratatui (0.30+), running on Rust 2021, consuming NMP v0.8 as a git dependency, with iOS sibling shell sharing nmp-app-29er core composition |
| [29er-tui](nouns/29er-tui.md) | 29er-tui | extracted | Ratatui-based terminal user interface application for Nostr NIP-29 group chat |
| [29er-tui-crate-tui-29er-package](nouns/29er-tui-crate-tui-29er-package.md) | 29er-tui crate (tui-29er package) | extracted | Rust Ratatui crate providing the terminal user interface application for 29er; package name is tui-29er (Rust forbids digit-starting names), binary name is 29er-tui, depends on nmp-app-29er as rlib (not FFI) |
| [29er-tui-tui-architecture](nouns/29er-tui-tui-architecture.md) | 29er-tui (TUI architecture) | extracted | A Ratatui app written in Rust that depends on nmp-app-29er as normal rlibs and receives Rust structs directly via .snapshot(), skipping the FFI/FlatBuffers boundary used by the Swift shell. |
| [boundedmessagemap](nouns/boundedmessagemap.md) | BoundedMessageMap | extracted | Generic map type for bounded message storage with MAX_PROJECTION_MESSAGES capped at 10,000 to prevent unbounded memory growth |
| [catppuccin-mocha](nouns/catppuccin-mocha.md) | Catppuccin Mocha | extracted | 29er TUI color palette: BASE #1e1e2e, TEXT #cdd6f4, LAVENDER #b4befe, MAUVE #cba6f7, BLUE #89b4fa, with semantic tokens for errors, warnings, success, and info |
| [chat-renderer](nouns/chat-renderer.md) | chat renderer | extracted | Slack-style grouped layout where consecutive messages from one author collapse under a single header (avatar dot + name + time), with left gutter, day dividers between calendar days, read-marker separator, auto-scroll, and new-message indicator |
| [claude](nouns/claude.md) | .claude/ | extracted | local worktree artifacts + local settings |
| [codex-exec](nouns/codex-exec.md) | codex exec | extracted | automated code review command that validates architecture (doctrine compliance, no business logic in shell) and implementation (no panics, test coverage); gates features before merge and provides feedback every 4 merged PRs |
| [compose-chat-message](nouns/compose-chat-message.md) | compose_chat_message | extracted | shared Rust function in nmp-app-29er that transforms raw text + mention pubkeys into kind:9 content with NIP-21 (nostr:npub1...) formatting + p-tags, called by both TUI and iOS before dispatch |
| [compose-chat-message-nmp-app-29er-compose-chat-message](nouns/compose-chat-message-nmp-app-29er-compose-chat-message.md) | compose_chat_message (nmp-app-29er::compose_chat_message) | extracted | A shared Rust function that transforms raw text + selected mention pubkeys into NIP-21 format (replacing @token with nostr:npub1...) and builds p-tags — the single authoritative home for mention composition, called by both TUI and Swift before dispatching to publish_group_event |
| [content-string-formatting-with-mentions](nouns/content-string-formatting-with-mentions.md) | content string formatting (with mentions) | extracted | belongs to the shell responsibility layer; the shell collects raw text and selected pubkeys, then either formats them or passes them to shared Rust composition |
| [content-string-responsibility](nouns/content-string-responsibility.md) | Content string responsibility | extracted | The content string (including mention text formatting like `nostr:npub1...`) is shell-owned. NMP's role with mentions is only to normalize `mention_pubkeys` into `["p", "<hex>"]` tags. Each shell (Swift and TUI) constructs its own mention text independently. |
| [contenttreewire](nouns/contenttreewire.md) | ContentTreeWire | extracted | serde-serializable FFI wire projection of ContentTree; flat index arena with u32 indices instead of recursive references |
| [discoveredgroup](nouns/discoveredgroup.md) | DiscoveredGroup | extracted | Data type representing a NIP-29 group with group_id, host_relay_url, name, picture, about, member_count, admin_count, public/open flags, optional parent, and children list |
| [discoveredgroupssnapshot](nouns/discoveredgroupssnapshot.md) | DiscoveredGroupsSnapshot | extracted | Snapshot type providing the current state of all discovered groups on a relay, with host_relay_url and groups list |
| [focus-stack](nouns/focus-stack.md) | Focus stack | extracted | An enum-based focus model with RoomList, MessageView, and Composer states, plus a FocusStack for modal overlays. Tab/Shift+Tab cycles between sections; Esc closes modals and returns focus. |
| [groupchatmessage](nouns/groupchatmessage.md) | GroupChatMessage | extracted | flat carrier where threading/reply nesting is deliberately not modeled; minimum fields needed for shell to draw a row |
| [groupchatsnapshot](nouns/groupchatsnapshot.md) | GroupChatSnapshot | extracted | Snapshot of group chat messages stored in newest-first order for reversed display in the TUI |
| [groupid](nouns/groupid.md) | GroupId | extracted | NIP-29 group identifier combining host_relay_url and local_id, instantiated via GroupId::new(relay, id) |
| [groupmember](nouns/groupmember.md) | GroupMember | extracted | represents a group member with pubkey, optional display_name, admin flag, and optional role |
| [groupmemberssnapshot](nouns/groupmemberssnapshot.md) | GroupMembersSnapshot | extracted | Snapshot of group membership with per-member pubkey, display_name, admin status, and role fields |
| [grouptreeprojection](nouns/grouptreeprojection.md) | GroupTreeProjection | extracted | folds kind:9 into per-group last-message preview + recursive unread counts, source of truth that must be reused not reimplemented |
| [hotlist](nouns/hotlist.md) | Hotlist | extracted | A priority-tiered notification system: Tier 1 (mentions, bold+count), Tier 2 (DMs, bold), Tier 3 (unread, bold), Tier 4 (activity, normal). Alt+A cycles through mentions first; read-marker separator shows last-read position. |
| [joinedgroupsprojection](nouns/joinedgroupsprojection.md) | JoinedGroupsProjection | extracted | NMP projection that emits is_member and is_admin boolean fields per active pubkey in each group; provides viewer's membership and admin status (reader truth) |
| [kernelevent](nouns/kernelevent.md) | KernelEvent | extracted | Nostr protocol event processed by NMP kernel with id, author, kind, created_at, tags, content, and relay_provenance for tracing event origin |
| [kerneleventobserver](nouns/kerneleventobserver.md) | KernelEventObserver | extracted | Trait for receiving kernel events with method on_kernel_event(&self, &KernelEvent); must be Send+Sync, lightweight, and panic-free |
| [kind-9](nouns/kind-9.md) | kind:9 | extracted | KIND_CHAT_MESSAGE — Nostr event kind for group chat messages |
| [kind-chat-message-kind-9](nouns/kind-chat-message-kind-9.md) | KIND_CHAT_MESSAGE (kind:9) | extracted | Nostr event kind integer 9; represents a group chat message in NIP-29 |
| [mention-content-format-nip-21](nouns/mention-content-format-nip-21.md) | Mention content format (NIP-21) | extracted | When a user selects a mention in the composer, the inserted text in the message content must be `nostr:npub1...` (bech32-encoded public key), not `@display_name`. The mention pubkey is separately added as a NIP-29 `p`-tag. |
| [nip-21](nouns/nip-21.md) | NIP-21 | extracted | Format for mentions in chat message content: nostr:npub1... (bech32-encoded pubkey) instead of @display_name |
| [nip-21-mention-format](nouns/nip-21-mention-format.md) | NIP-21 mention format | extracted | Mention references in message content are nostr:npub1... (bech32-encoded public key), not @display_name |
| [nip-29](nouns/nip-29.md) | NIP-29 | extracted | Nostr relay groups that serve as chat rooms/channels |
| [nmp](nouns/nmp.md) | NMP | extracted | Rust event-processing kernel for Nostr event processing with hard separation: business logic (kinds, tags, signing, relay routing, NIP-29, unread aggregation) lives in Rust; shells only render |
| [nmp-app-29er](nouns/nmp-app-29er.md) | nmp-app-29er | extracted | the 29er-specific glue crate in 29er repo (not vendored), re-exports whole 29er surface, contains GroupTreeProjection for unread + last-message preview, exposes shared composition (post_chat_message) via FFI |
| [nmp-content](nouns/nmp-content.md) | nmp-content | extracted | Layer A substrate producing ContentTree/Segment IR with FlatBuffers wire format for shells |
| [nmp-content-tokenize-text](nouns/nmp-content-tokenize-text.md) | nmp_content_tokenize_text | extracted | pure content-tokenizer C-ABI; FFI wrapper around nmp-content's tokenizer; does not resolve entities |
| [nmp-core](nouns/nmp-core.md) | nmp-core | extracted | NMP kernel component providing KernelEvent, KernelEventObserver trait, actor/command substrate, BoundedMessageMap, projection registration, and DispatchEnvelope |
| [nmp-defaults](nouns/nmp-defaults.md) | nmp-defaults | extracted | Canonical NMP composition implementing NIP-02/17/57/65 routing substrate and relay network topology, registered once at kernel initialization |
| [nmp-doctrine](nouns/nmp-doctrine.md) | NMP doctrine | extracted | kernel emits, per-app crate composes, shell only renders |
| [nmp-ffi](nouns/nmp-ffi.md) | nmp-ffi | extracted | Component owning the NmpApp handle, providing lifecycle methods (nmp_app_new/start/stop/free), identity management (nmp_app_signin_nsec), relay operations (nmp_app_add_relay), and inherent methods for observer/projection registration |
| [nmp-nip29](nouns/nmp-nip29.md) | nmp-nip29 | extracted | NIP-29 relay groups (chat rooms/channels), the core domain crate |
| [nmp-nip29-publish-group-event](nouns/nmp-nip29-publish-group-event.md) | nmp.nip29.publish_group_event | extracted | Generic NIP-29 publish surface where shell provides (kind, content, tags) and NMP injects group envelope (h-tag, previous refs, relay routing) then signs and publishes |
| [nmp-nostr-multi-platform](nouns/nmp-nostr-multi-platform.md) | NMP (Nostr Multi-Platform) | extracted | a Rust event-processing kernel with the hard rule: kernel emits, per-app crate composes, shell only renders; ALL Nostr business logic (event kinds, signing, tags, relay routing, NIP semantics, unread aggregation) lives in Rust |
| [nmp-planner](nouns/nmp-planner.md) | nmp-planner | extracted | Component providing LogicalInterest, InterestId, and InterestLifecycle mechanisms that drive which relay subscriptions are opened and managed |
| [nostr-content](nouns/nostr-content.md) | nostr_content | extracted | App-owned native components vendored from the NMP registry |
| [nostrcontentview](nouns/nostrcontentview.md) | NostrContentView | extracted | SwiftUI renderer for ContentTreeWire that walks tree.roots and flattens the arena into block-level groups with inline text concatenation |
| [nucleo-matcher](nouns/nucleo-matcher.md) | nucleo-matcher | extracted | Fuzzy matching library used in command palette with optimization to hoist matcher construction out of hot path for performance |
| [outbox](nouns/outbox.md) | Outbox | extracted | TUI strip showing message publication states: Pending ⏳ / Confirmed ✓ / Failed ✗, with [Ctrl-R] to retry |
| [publish-group-event](nouns/publish-group-event.md) | publish_group_event | extracted | the canonical NMP action surface (nmp.nip29.publish_group_event): app says 'publish (kind, content, tags) to group X', NMP injects envelope (h, previous, routing) and signs |
| [publish-group-event-nmp-nip29-publish-group-event-action](nouns/publish-group-event-nmp-nip29-publish-group-event-action.md) | publish_group_event (nmp.nip29.publish_group_event action) | extracted | The generic kind-agnostic NIP-29 publish surface through which shells dispatch any event (kind, content, tags) to a group — NMP-nip29 injects the envelope (h/previous/host-relay routing) and signs, superseding the deleted kind-specific PostChatMessageAction |
| [ratatui](nouns/ratatui.md) | Ratatui | extracted | Rust TUI framework, version 0.30, with a widget ecosystem (tui-textarea, tui-scrollview, tui-widget-list, ratatui-themes for Catppuccin theming) |
| [ratatui-markdown](nouns/ratatui-markdown.md) | ratatui-markdown | extracted | Markdown rendering widget with tree-sitter syntax highlighting for rich message content and collapsible JSON/TOML display |
| [ratatui-themes](nouns/ratatui-themes.md) | ratatui-themes | extracted | Theming engine with 15+ curated themes (Catppuccin, Nord, Dracula, Gruvbox, Tokyo Night) and semantic error/warning/success/info colors |
| [relay-browsing-feature](nouns/relay-browsing-feature.md) | relay browsing feature | extracted | relay selector implementation with Rust RelaySelectorProjection and iOS RelaySelectorSheet, allowing users to select, add, and remove relays via NIP-51 kind:30002 |
| [relayselectorprojection](nouns/relayselectorprojection.md) | RelaySelectorProjection | extracted | Rust projection that reads user's NIP-51 kind:30002 relay set, tracks the active relay, publishes changes back, and falls back to the default operator relay when no NIP-51 list exists |
| [relayselectorsheet](nouns/relayselectorsheet.md) | RelaySelectorSheet | extracted | SwiftUI List sheet component for selecting a relay, adding new relay via URL field, and swipe-deleting NIP-51-sourced relays |
| [rich-content-rendering](nouns/rich-content-rendering.md) | rich content rendering | extracted | replace raw-content Text with NMP content renderer so mentions/embeds/media/hashtags/markdown render properly |
| [screen](nouns/screen.md) | Screen | extracted | enum state machine with variants: Login (nsec paste + NMP initialization) and App (main chat UI); models the TUI's top-level view flow |
| [segment-ir](nouns/segment-ir.md) | Segment IR | extracted | internal, ergonomic, recursive representation that is NOT serde-serializable; projects to ContentTreeWire wire form |
| [shell-in-nmp-architecture](nouns/shell-in-nmp-architecture.md) | Shell (in NMP architecture) | extracted | the component that only renders (does not handle business logic, signing, or relay routing) |
| [shell-tui-ios](nouns/shell-tui-ios.md) | shell (TUI/iOS) | extracted | A rendering and input layer that collects raw text, mention selections, and keyboard input; dispatches to the Rust core (nmp-app-29er → nmp-nip29) — owns only view state (focus, scroll, UI state), not protocol logic, event construction, signing, relay selection, membership truth, or content parsing |
| [slack-layout](nouns/slack-layout.md) | Slack layout | extracted | pure presentation layer: group consecutive messages by author with one header per run, day dividers, avatar/name gutter, aligned timestamps |
| [slack-style-chat-renderer](nouns/slack-style-chat-renderer.md) | Slack-style chat renderer | extracted | feature work on both TUI and iOS comprising TUI components::nostr_content, iOS Components/NostrContent + Components/NostrUser, profile bridge, and wiki docs |
| [swift-app](nouns/swift-app.md) | Swift app | extracted | pure renderer over the C-ABI + FlatBuffers boundary |
| [the-mention-bug](nouns/the-mention-bug.md) | the mention bug | extracted | mentions in chat content should be formatted as nostr:npub1... (NIP-21 Nostr identifier URIs), not @display_name text; affects both TUI and iOS independently until logic moves to shared Rust |
| [tokenizer](nouns/tokenizer.md) | tokenizer | extracted | render substrate (Layer A) |
| [tui](nouns/tui.md) | TUI | extracted | pure renderer + input layer over NMP snapshots, mirroring the Swift shell |
| [tui-29er](nouns/tui-29er.md) | tui-29er | extracted | Rust package name for 29er Ratatui TUI (digits-forbidden, so named tui-29er; binary named 29er-tui) |
| [tui-29er-ratatui-tui](nouns/tui-29er-ratatui-tui.md) | TUI (29er Ratatui TUI) | extracted | Pure renderer and input layer over NMP snapshots, mirroring the Swift app's shell architecture without replicating business logic |
| [tui-29er-tui](nouns/tui-29er-tui.md) | TUI (29er-tui) | extracted | A buildable terminal app target for 29er using Ratatui with a login screen, hierarchical message rendering with per-author colors, composer with @mention popup and publish outbox, command palette, and membership/admin actions — consumes nmp-app-29er as an rlib, not FFI |
| [tui-logger](nouns/tui-logger.md) | tui-logger | extracted | Live log pane widget with bounded ring buffers (1k/10k capacity) and interactive log level filtering |
| [tui-popup-tui-overlay](nouns/tui-popup-tui-overlay.md) | tui-popup/tui-overlay | extracted | Modal and popup layer widget for reply dialogs, confirmations, profile cards, and error toasts |
| [tui-scrollview](nouns/tui-scrollview.md) | tui-scrollview | extracted | Scrollable viewport container widget providing automatic scrollbars and offset state for message history rendering |
| [tui-textarea](nouns/tui-textarea.md) | tui-textarea | extracted | Multi-line text input widget for message composer, supporting cursor movement, selection, undo/redo, emacs/vim keymaps, and search |
| [tui-tree-widget](nouns/tui-tree-widget.md) | tui-tree-widget | extracted | Collapsible hierarchy widget with expand/collapse state and identifier-path tracking that survives item reordering |
| [tui-widget-list](nouns/tui-widget-list.md) | tui-widget-list | extracted | List widget with per-item variable height and built-in scrolling/selection state, suited for multiline message bubble rendering |
| [tuisnapshot](nouns/tuisnapshot.md) | TuiSnapshot | extracted | TUI state struct containing channel_tree, selected_channel_id, selected_messages, selected_members, is_admin, my_pubkey, publish_outbox, identity_state, relay_state, errors, selected_index, focus, message_scroll, palette_open, active_form, login_error, and screen |

