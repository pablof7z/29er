---
type: research-record
date: 2026-06-26
session: 00700b1c-4fd7-4a41-90e9-7a76d64b42cc
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-29er/00700b1c-4fd7-4a41-90e9-7a76d64b42cc.jsonl
source_lines: 2814-2815
agent_attribution: main
has_preregistered_criteria: true
has_method: true
has_structured_report: true
characterization: "Implementation code review verdict: Opus approves 8 doctrine fixes, Codex blocks on 2 additional violations (Rust raw-tap doctrine violation, Join-button regression)"
captured_at: 2026-06-26T12:13:03Z
---

Implementation code review verdict: Opus approves 8 doctrine fixes, Codex blocks on 2 additional violations (Rust raw-tap doctrine violation, Join-button regression)

---

User: [Agent task result: Dynamic workflow "Fix all 8 NMP doctrine violations in 29er (nmp-app-29er + iOS Swift), codex+opus review, land on main" completed]
{"status":"blocked-on-review","opus":"APPROVE","opusBlocking":[],"codexProceed":false,"codexIssues":["BLOCKING (confirmed) - NMP doctrine violation in crates/nmp-app-29er/src/ffi.rs. open_group_discovery_with_tree (line 363+) manually registers DiscoveredGroupsProjection, GroupTreeProjection, and JoinedGroupsProjection via app.register_live_event_tap over raw KernelEvents (lines 367-408), and line 458-460 opens a raw global interest filter `{\"kinds\":[9]}` directly via app.open_interest instead of NMP's hydrating open_group_discovery / open_joined_groups / open_group_chat doors. group_tree.rs:196 (on_kernel_event) re-derives kind/h state from raw events (filters on event.kind != KIND_CHAT_MESSAGE, reads h_tag_value). This is a real Rust-side doctrine violation and the central subject of task #11 (fix NMP doctrine violations).","HIGH (confirmed) - Functional regression. nmp_app_29er_select_group_members (ffi.rs:533) is a validated no-op and the app crate registers NO `nmp.nip29.group_members` typed snapshot projection (grep over crates/nmp-app-29er/src finds only the no-op FFI fn). On the Swift side KernelModel+Apply.swift:133 returns `typedGroupMembers ?? .empty`, and GroupMembersSnapshot.empty has groupId == nil. GroupTreeView.swift:267 `membersProjectionMatchesGroup` is `model.groupMembers.groupId == groupId`, so it can never become true. That permanently shows membershipStatusLabel = \"Checking\" (line 318) and disables the Join button (line 558 `!membersProjectionMatchesGroup`), leaving users unable to join."],"branch":"feat/swift-doctrine-fixes"}
