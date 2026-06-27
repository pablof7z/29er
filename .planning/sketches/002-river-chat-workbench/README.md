---
sketch: 002
name: river-chat-workbench
question: "Can the TUI feel like a smooth conversation river with contextual side drawers instead of a dashboard?"
winner: null
tags: [tui, chat, drawer, composer, nip29, ratatui]
---

# Sketch 002: River Chat Workbench

## Design Question
Can 29er center the TUI around a readable kind:9 message river while keeping room navigation, membership, and admin workflows available as contextual drawers instead of persistent dashboard panels?

## What To Look For
- Whether the full-height message stream feels calm and conversational enough to be the primary workspace.
- Whether the top rail carries enough room state: open/closed, member/admin, relay, unread, and projection counts.
- Whether drawers feel like buildable Ratatui sheets for room switching, member review, and admin actions.
- Whether outbox status, retry, join gating, and @mentions are visible without turning the UI into diagnostics.
- Whether the keyboard model is clear: `R` rooms, `M` members, `A` admin, `Esc` close, `Tab` accept mention, `Ctrl+Enter` send.

## Ratatui Mapping
- `Layout`: root top rail, message river, composer, and slide-in drawer regions.
- `List`: hierarchical GroupTreeNode room switcher and GroupMember drawer rows.
- `Table`: PublishOutboxItem/admin action status table.
- `Tabs`: member drawer filters.
- `Paragraph`: message bodies, room previews, status copy, and action descriptions.
- `Scrollbar`: message river and drawer bodies.
- `Gauge` or `Sparkline`: future relay/publish confidence in the admin sheet.
- `ratatui-textarea`: multiline composer with @mention insertion.
- `throbber-widgets-tui`: loading/sync indicators for relay and projection refresh.
- `tachyonfx`: smooth drawer open/close and pending-to-confirmed transitions.
- `ratatui-image`: optional future avatar rendering, not required for this direction.

## Source Of Truth Boundary
The TUI should render Rust/NMP projections such as `GroupTreeNode`, `GroupChatMessage`, `GroupMember`, and `PublishOutboxItem`. It should dispatch typed actions for publish, join/leave, invite creation, role changes, child-room creation, and parent moves. It should not derive protocol truth, retry policy, signing state, membership state, or transport state locally.

## How To View
Open `.planning/sketches/002-river-chat-workbench/index.html` in a browser.
