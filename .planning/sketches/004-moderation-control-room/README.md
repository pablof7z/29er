---
sketch: 004
name: moderation-control-room
question: "Can membership/admin work become a polished control-room workflow without hiding the chat loop?"
winner: null
tags: [tui, admin, moderation, members, invites, hierarchy, outbox]
---

# Sketch 004: Moderation Control Room

## Design Question

Can 29er admins manage invites, member roles, child rooms, hierarchy moves, and publish retries from a dense Ratatui operation console while keeping the normal kind:9 chat loop visible?

## How to View

Open `.planning/sketches/004-moderation-control-room/index.html`.

## What to Look For

- Whether the three-pane control-room layout keeps the room tree, active timeline, and admin workflow understandable at terminal density.
- Whether action tabs for Invites, People, Room, and Move feel like typed operations over Rust/NMP projections rather than local TUI-owned truth.
- Whether the outbox makes pending publish, failure, and retry state visible enough for real admin work.
- Whether selecting members and moving rooms gives enough preview before dispatching irreversible admin actions.

## Ratatui Mapping

- `Layout`: persistent room tree, chat timeline, and operation console panes.
- `List`: hierarchical `GroupTreeNode` room navigation with unread counts.
- `Table`: `GroupMember` admin/member table and `PublishOutboxItem` queue.
- `Tabs`: Invites, People, Room, and Move action forms.
- `Paragraph`: chat messages, projection dump, move preview, and composer copy.
- `Scrollbar`: room list, chat timeline, member table, form panel, and outbox regions.
- `Gauge` / `Sparkline`: publish confidence and relay acknowledgement telemetry in the outbox header area.
- `ratatui-textarea`: message composer and invite notes.
- `tui-input`: single-line groupId, pubkey, role, and move reason fields.
- `throbber-widgets-tui`: pending publish indicators.
- `tachyonfx`: modal/form focus transitions for admin operations.
