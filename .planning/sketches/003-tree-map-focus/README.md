---
sketch: 003
name: tree-map-focus
question: Can a spatial tree-map and focus stack make the NIP-29 hierarchy understandable at terminal density?
winner: null
tags:
  - tui
  - hierarchy
  - map
  - focus
---

# Tree Map Focus

## Design Question

Can the hierarchy itself become the main navigation surface, so a signed-in 29er user understands parent rooms, child rooms, unread propagation, and the selected kind:9 timeline without falling back to a flat sidebar?

## What To Look For

- Whether the ASCII room map makes parent/child relationships clear at a glance.
- Whether unread rollups explain why a parent branch needs attention.
- Whether every selected branch or leaf feels like a real room with its own timeline and composer.
- Whether admin work such as `SetParent` feels contextual instead of hidden in a separate settings screen.
- Whether the TUI remains projection-only: Rust/NMP owns protocol, state, signing, transport, membership/admin derivation, retry policy, and publish/outbox state.

## Ratatui Mapping

- `Layout`: outer map/chat/projection split plus status bars.
- `List`: navigable ASCII tree rows and kind:9 timeline messages.
- `Table`: `GroupTreeNode`, `GroupMember`, and admin action detail rows.
- `Tabs`: top-level rooms/members/outbox/relay modes.
- `Paragraph`: selected room hero, annotations, empty/loading/error states, composer hints.
- `Scrollbar`: tree and timeline panes when the hierarchy or chat grows.
- `Gauge` or `Sparkline`: descendant unread propagation and relay confidence.
- `tui-big-text`: large selected room/state label.
- `tachyonfx`: focus transition when moving parent/sibling/child.
- `throbber-widgets-tui`: loading subscription state.
- `tui-widgets`: tree/list helpers for the hierarchy surface if they fit the final implementation.
