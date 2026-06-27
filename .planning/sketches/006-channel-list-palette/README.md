---
sketch: 006
name: channel-list-palette
question: "Is a simple channel list plus command palette enough for the first 29er TUI?"
winner: null
tags: [tui, channels, palette, simple]
---

# Sketch 006: Channel List Palette

## Design Question

Is the best first Ratatui shape just a focused list of channels, with every secondary action living in a fast command palette?

## How To View

Open `.planning/sketches/006-channel-list-palette/index.html`.

## What To Look For

- Whether the channel list carries enough context: hierarchy, unread count, membership state, last message, and pending publish.
- Whether the palette feels like the right place for jump, search, compose, join/leave, members, and admin actions.
- Whether this is simpler without becoming underpowered.

## Ratatui Mapping

- `Layout`: top status row, single channel-list viewport, bottom hint row.
- `List`: focused `GroupTreeNode` channel list.
- `Paragraph`: selected channel summary and palette result descriptions.
- `Table`: optional palette result rendering for action metadata.
- `ratatui-textarea`: palette query input and future inline composer.
- `Scrollbar`: channel list and palette results.
- `tachyonfx`: subtle palette open/close and selection feedback.
