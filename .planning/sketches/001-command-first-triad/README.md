---
sketch: 001
name: command-first-triad
question: "Can a command-first three-pane layout make room navigation, chat, and actions fast without feeling cramped?"
winner: null
tags: [tui, command, panes, keyboard]
---

# Sketch 001: Command-first Triad

## Design Question

Can a command-first three-pane layout keep the NIP-29 room tree, current kind:9 timeline, and membership/admin actions available at the same time without making the terminal feel cramped?

## What To Look For

- The center timeline stays stable while room navigation and command actions change around it.
- The right pane behaves like a contextual typed-action surface, not local app state.
- Room filtering, room selection, command palette tabs, message send/confirm, mention suggestions, and outbox retry all produce visible feedback.
- Status is present but compact: signed-in membership, admin count, unread count, publish pending/failed/published, and relay confidence.

## Ratatui Mapping

- `Layout`: outer terminal rows plus three-pane horizontal split.
- `List`: searchable hierarchical `GroupTreeNode` room tree.
- `Paragraph`: timeline message rows, room metadata, keyboard hint strip, command descriptions.
- `Tabs`: command modes for room, admin, and outbox contexts.
- `Table`: outbox items and member/admin rows in the right pane.
- `Scrollbar`: tree, timeline, command palette, and outbox overflow.
- `ratatui-textarea`: kind:9 composer with inline mention completion.
- `throbber-widgets-tui`: pending publish and relay ack states.
- `tui-big-text`: possible future empty/loading/error status moments.
- `tachyonfx`: small focus, palette, and publish confirmation transitions.

## How To View

Open `.planning/sketches/001-command-first-triad/index.html` in a browser.
