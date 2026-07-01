---
title: Relay Selection and Status
slug: relay-selection
topic: relay-selection
summary: Relay selection, default relay, and fallback policy are owned by Rust/NMP state and actions; the TUI renders relay state and dispatches selection intent.
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-06-26
updated: 2026-07-01
verified: 2026-07-01
compiled-from: conversation
sources:
  - session:019f02cc-8e3e-7782-9ff3-73e2a3824267
---

# Relay Selection and Status

## Architecture

Relay selection, default relay, and fallback policy are owned by Rust/NMP state and actions. iOS consumes the `nmp.29er.relay_selector` projection and calls relay-selector actions; the TUI pre-fills its login relay field from `nmp-app-29er` configuration and treats edits as explicit user selection. Neither shell owns a hidden default relay literal. <!-- [^019f0-c82fa] -->

## Connection Status

Relay connection status reflects NMP relay diagnostics and connection state, not heuristics based on data arrival history. <!-- [^019f0-c08cd] -->
