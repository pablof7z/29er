---
sketch: 005
name: relay-aware-flow
question: Can relay health, subscriptions, and publish confidence be first-class without overwhelming normal users?
winner: null
tags: [tui, relay, diagnostics, outbox]
---

# Relay-Aware Flow

## Design Question

Can a Ratatui TUI keep relay health, subscription progress, and publish confidence always visible while preserving the ordinary NIP-29 chat loop: room tree, kind:9 timeline, @mentions, and composer?

## What To Look For

- The relay/sync layer is always present, but the center of gravity remains room navigation and chat.
- Offline publishing is understandable: composing while offline creates queued outbox items, and reconnecting flushes them.
- Protocol states feel like projections from Rust/NMP, not local TUI-owned truth.
- Membership/admin capabilities are visible as typed actions without turning the screen into an admin-only dashboard.
- The layout stays buildable as Ratatui panes, lists, tables, gauges, paragraphs, tabs, scrollbars, and a textarea composer.

## Ratatui Mapping

- `Layout`: top bar, three-pane workspace, and bottom relay console.
- `List`: hierarchical `GroupTreeNode` room tree and event log.
- `Table`: relay detail, subscription rows, and `PublishOutboxItem` status rows.
- `Tabs`: rooms, mentions, admin, and outbox mode affordances.
- `Paragraph`: chat messages, room path, status copy, and relay detail text.
- `Gauge`: subscription progress and sync confidence.
- `Sparkline`: would fit relay latency history in the live ops rail.
- `Scrollbar`: room tree, timeline, live ops rail, and console sections.
- `ratatui-textarea`: composer with @mention text input.
- `tui-big-text`: large connection state in the bottom console.
- `throbber-widgets-tui`: syncing and reconnecting indicators.
- `tachyonfx`: calm status pulses for relay transitions and publish retry feedback.
