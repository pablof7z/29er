---
title: TUI Snapshots and State Management
slug: snapshot-system
topic: snapshot-system
summary: The 29er app sources group discovery, membership, and chat state via NMP's open_group_discovery, open_joined_groups, and open_group_chat APIs
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-06-26
updated: 2026-06-28
verified: 2026-06-26
compiled-from: conversation
sources:
  - session:019f02cc-8e3e-7782-9ff3-73e2a3824267
  - session:019f03c8-721e-7783-87f1-b3dbc19bfcef
  - session:019f0d0b-8033-7cf0-a750-21b389b8daf0
---

# TUI Snapshots and State Management

## Snapshot Data Sources

The 29er app sources group discovery, membership, and chat state via NMP's open_group_discovery, open_joined_groups, and open_group_chat APIs. The TUI decodes and renders NMP snapshots from `nmp.29er.group_tree` and `nmp.nip29.*` without composing projections; projection logic is owned by Rust/NMP. The group_members projection, produced by Rust app code, is used by GroupTreeView.swift's join and composer state logic.

<!-- citations: [^019f0-4dd62] [^019f0-e996c] -->
## Update Mechanism

TUI snapshots are driven by NMP update callbacks and snapshot frames rather than polling. <!-- [^019f0-f0bd0] -->

## Observer Lifecycle

GroupChatProjection observers are retained only for the active view; previous observers are closed when navigating to other channels. <!-- [^019f0-2ec56] -->

## Selection State

Room selection is a single snapshot-owned fact; tree navigation and all selection commands (`g`, `G`, `Alt+A`) maintain consistent state. <!-- [^019f0-5c46a] -->

## Test Suite

The tui-29er test suite compiles with test snapshot builders reflecting the current `TuiSnapshot` and `ChannelListItem` schema. <!-- [^019f0-5e777] -->

## Reset Local Database

The Reset Local Database action clears the group discovery session and rehydrates the UI with current relay data, ensuring stale channels no longer appear. <!-- [^019f0-597b2] -->
