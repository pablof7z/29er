---
type: research-record
date: 2026-06-27
session: 2d0bcf1a-c9a6-4929-99bd-7956ebfabf16
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-29er/2d0bcf1a-c9a6-4929-99bd-7956ebfabf16.jsonl
source_lines: 1263-1361
agent_attribution: afeeb8bbacdf85127
has_preregistered_criteria: true
has_method: true
has_structured_report: true
characterization: "Audit of all git stashes and worktrees against main; identified relay selector feature as critical missing in stash@{0}, membership popup feature in worktree, superseded items, and historical obsolete code; provided restore/cherry-pick/discard verdicts"
captured_at: 2026-06-27T18:53:33Z
---

Audit of all git stashes and worktrees against main; identified relay selector feature as critical missing in stash@{0}, membership popup feature in worktree, superseded items, and historical obsolete code; provided restore/cherry-pick/discard verdicts

---

User: [Agent task result: Agent "Audit all branches/stashes for features missing from main" finished]
Confirmed: no relay selector code exists in the current main. Here is the full audit:

---

## Audit Report: Work Not in Main

### CRITICAL MISSING: stash@{0} — Complete Relay Browser/Switcher

**Location:** `stash@{0}` ("review-stash" on `feat/swift-doctrine-fixes`, based on `6db5dac` which IS in main)

**What it contains — 32 files, 1960 insertions:**

This is the relay browsing feature. It is entirely absent from main.

**Rust side (untracked in stash@{0}^3):**
- `crates/nmp-app-29er/src/relay_selector.rs` — 454-line `RelaySelectorProjection` that reads the user's NIP-51 kind:30002 relay set, tracks the active relay, publishes changes back, falls back to the default operator relay when no NIP-51 list exists yet. Has full unit tests.
- `crates/nmp-app-29er/schema/relay_selector.fbs` — FlatBuffers schema (`N29R` file identifier)
- `crates/nmp-app-29er/src/wire/generated/relay_selector_generated.rs` — generated FB code

**Rust FFI (`ffi.rs` diff):**
- Three new C-ABI functions: `nmp_app_29er_relay_selector_select_relay`, `nmp_app_29er_relay_selector_add_relay`, `nmp_app_29er_relay_selector_remove_relay`
- `TwentyNinerHandle` gets a `relay_selector: Arc<RelaySelectorProjection>` field

**iOS Swift side (tracked diff):**
- `GroupTree.swift` — adds `RelaySelectorRow`, `RelaySelectorSnapshot`, `RelayDiagnosticsRelay`, `RelayDiagnosticsSnapshot`, `String.relayHostLabel` extension
- `NmpCore.h` — declares the three new C-ABI relay selector functions
- `KernelBridge.swift` — adds `selectNip29Relay()`, `addNip29Relay()`, `removeNip29Relay()`
- `KernelModel.swift` — adds `@Published var typedRelaySelector`, `@Published var typedRelayDiagnostics`, plus the three intent methods forwarded to `KernelHandle`
- `KernelUpdateTypes.swift` — adds `typedRelaySelector` and `typedRelayDiagnostics` to `KernelUpdateResult`
- `KernelModel+Apply.swift` — applies relay selector snapshot on each tick; replaces the old `groupDefaults.suggestedRelayUrl` path with `relaySelector.activeRelayUrl`; adds `relaySelector`, `relayDiagnostics`, `activeRelayTitle`, `relayDisplayName(for:)` computed properties
- `TypedProjectionDecoders.swift` — adds `TypedRelaySelectorDecoder` and `TypedRelayDiagnosticsDecoder`
- `TypedProjectionGlue.swift` — adds `relaySelector()` and `relayDiagnostics()` glue functions
- `GroupTreeView.swift` — the navigation bar title becomes a tappable `Button` showing the active relay's NIP-11 name (or hostname fallback); tapping opens `RelaySelectorSheet`, a SwiftUI `List` sheet letting the user select a relay (auto-dismisses), add a new relay via URL field, and swipe-delete NIP-51-sourced relays

**iOS untracked (in stash@{0}^3):**
- `relay_selector_generated.swift` and `relay_diagnostics_generated.swift` — FlatBuffers generated Swift bindings referenced from `project.pbxproj`

**Recommendation: RESTORE.** This is the relay browsing UI the user says was lost. It is a complete, tested implementation. The stash needs to be applied/restored: the tracked diff goes on top of `6db5dac`, and the untracked files (`relay_selector.rs`, generated files, `.planning/sketches/*`, `docs/wiki/AGENTS.md`) need to be extracted from `stash@{0}^3`.

---

### NOT IN MAIN #2: `worktree-wf_91de0153-656-29` — TUI Popup for Membership Forms

**Commit:** `3e90f7d  ux(tui): tui-popup for membership forms, focus trap, destructive confirms`

**What it does:** Replaces the manual `Layout`+`Clear` centering in the membership overlay with the `tui-popup = "0.7"` crate (`Popup::new(body)`), adds an `error` field displayed inline inside the popup, and adds a focus trap (Tab/Down/Up cycle inside popup only, Esc restores focus stack). Adds `error_clears_on_form_change` test.

**Genuinely missing?** Yes — current `membership.rs` in main still uses manual `Layout`/`Clear`, has no `error` field, and no `tui-popup` dependency in `Cargo.toml`.

**Recommendation: Cherry-pick `3e90f7d` onto main** (or the current working branch). It's a self-contained UX improvement with no conflicts expected.

---

### SUPERSEDED: `feat/tui-epic-full` — NIP-21 TUI Mention

**Commit:** `31d882e  fix(tui): mention inserts nostr:npub1... (NIP-21) instead of @display_name`

**Why superseded:** Main's commit `363a310` moved NIP-21 mention composition into the shared `nmp-app-29er` Rust layer (the correct "doctrine" approach). The TUI now sends raw pubkeys and the shared Rust converts them. The `31d882e` approach of doing it inside the TUI's `composer.rs` is the wrong layer.

**Recommendation: Discard.** Already superseded correctly.

---

### SUPERSEDED: stashes 1-9

- **stash@{1}:** WIP that became `31d882e` — same NIP-21 TUI fix, superseded by main.
- **stashes 2-8:** Only `Cargo.lock` changes (21 lines each), different lockfile states from old branches. Discard.
- **stash@{9}:** Large Cargo.lock diff from very early `feat/issue-2-tui-integration` branch. Discard.

---

### HISTORICAL: stash@{10} — Pre-NMP Swift Code

**Base:** `main` before NMP integration. Contains `Sources/Models/KernelModel.swift`, `Sources/Services/RelayService.swift`, `Sources/Views/ContentView.swift` — the old pure-Swift relay implementation.

**Recommendation: Discard.** This is the pre-NMP architecture, entirely superseded.

---

### Everything else: In Main

- `fix/session-persistence` commit `3781223` — **in main** ✓
- `milestone/M001` — **in main** ✓
- `feat/ratatui-0.30-upgrade-wt` — **in main** ✓
- All `feat/ux-*` branches (`onboarding`, `hotlist`, `optimistic-send`, `focus-stack`, `palette`) — **in main** ✓
- All `fix/A/B/C/D-*` branches — **in main** ✓

---

## Action Summary

| Priority | Item | Action |
|----------|------|--------|
| 1 — URGENT | stash@{0} relay browser (iOS + Rust) | Restore the stash, commit the relay selector feature |
| 2 | `3e90f7d` TUI popup membership | Cherry-pick onto main/working branch |
| 3 | `31d882e`, stashes 1-9, stash@{10} | Drop/discard |

The relay browsing feature is entirely in `stash@{0}`. To recover it: `git stash pop stash@{0}` will restore all 32 tracked files; then separately recover the untracked files (`relay_selector.rs`, FlatBuffers generated files) from `stash@{0}^3` using `git show stash@{0}^3:<path>` for each file.
