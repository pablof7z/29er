# Sketch Manifest

## Design Direction
Explore a Ratatui TUI for 29er as a dense, smooth, keyboard-first terminal app. The sketches should respect the real product shape: NIP-29 hierarchical rooms, every room as a real kind:9 timeline, member/admin state from Rust-owned projections, typed actions for join/leave/admin work, and publish/outbox confidence. The HTML files are interactive wireframes only, but each direction should map to buildable Ratatui layouts and ecosystem widgets.

## Reference Points
- Ratatui built-in widgets: Layout, List, Table, Tabs, Paragraph, Gauge, Sparkline, BarChart, Scrollbar.
- Ratatui ecosystem candidates: ratatui-textarea, tui-big-text, throbber-widgets-tui, ratatui-image, tachyonfx, tui-widgets, color-eyre driven diagnostics.
- 29er source of truth: Rust/NMP owns protocol, state, signing, transport, membership/admin derivation, publish retry, and outbox state. TUI shell renders projections and dispatches typed actions.

## Sketches

| # | Name | Design Question | Winner | Tags |
|---|------|----------------|--------|------|
| 001 | command-first-triad | Can a command-first three-pane layout make room navigation, chat, and actions fast without feeling cramped? | null | [tui, command, panes, keyboard] |
| 002 | river-chat-workbench | Can the TUI feel like a smooth conversation river with contextual side drawers instead of a dashboard? | null | [tui, chat, drawer, composer] |
| 003 | tree-map-focus | Can a spatial tree-map and focus stack make the NIP-29 hierarchy understandable at terminal density? | null | [tui, hierarchy, map, focus] |
| 004 | moderation-control-room | Can membership/admin work become a polished control-room workflow without hiding the chat loop? | null | [tui, admin, moderation, outbox] |
| 005 | relay-aware-flow | Can relay health, subscriptions, and publish confidence be first-class without overwhelming normal users? | null | [tui, relay, diagnostics, outbox] |
| 006 | channel-list-palette | Is a simple channel list plus command palette enough for the first 29er TUI? | null | [tui, channels, palette, simple] |
