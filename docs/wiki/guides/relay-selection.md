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
updated: 2026-06-26
verified: 2026-06-26
compiled-from: conversation
sources:
  - session:019f02cc-8e3e-7782-9ff3-73e2a3824267
---

# Relay Selection and Status

## Architecture

Relay selection, default relay, and fallback policy are owned by Rust/NMP state and actions; the TUI renders relay state and dispatches selection intent. <!-- [^019f0-c82fa] -->

## Connection Status

Relay connection status reflects NMP relay diagnostics and connection state, not heuristics based on data arrival history. <!-- [^019f0-c08cd] -->
