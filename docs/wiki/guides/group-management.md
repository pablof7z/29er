---
title: Group Management
slug: group-management
topic: group-management
summary: Join, leave, and admin action forms consume dispatch error results before displaying success/failure status.
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

# Group Management

## Action Forms

Join, leave, and admin action forms consume dispatch error results before displaying success/failure status. <!-- [^019f0-42e6f] -->

## Group ID and Policy

Group ID generation and default policy (public/open) are controlled by Rust/NMP actions, not hardcoded in the TUI. <!-- [^019f0-f2921] -->
