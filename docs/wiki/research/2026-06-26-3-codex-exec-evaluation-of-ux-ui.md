---
type: research-record
date: 2026-06-26
session: 00700b1c-4fd7-4a41-90e9-7a76d64b42cc
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-29er/00700b1c-4fd7-4a41-90e9-7a76d64b42cc.jsonl
source_lines: 1363-1364
agent_attribution: main
has_preregistered_criteria: true
has_method: true
has_structured_report: true
characterization: "Codex exec evaluation of UX/UI improvements workflow: 7 blocking issues identified (test compilation, mentions dispatch, outbox confirmation, observer leak, NMP architecture violations, relay state), verdict needs-fixes"
captured_at: 2026-06-26T07:34:04Z
---

Codex exec evaluation of UX/UI improvements workflow: 7 blocking issues identified (test compilation, mentions dispatch, outbox confirmation, observer leak, NMP architecture violations, relay state), verdict needs-fixes

---

User: [Agent task result: Dynamic workflow "UX/UI improvements: tree widget, scrollview, palette, status bar, focus stack, optimistic send, hotlist" completed]
{"status":"needs-fixes","issues":["Test suite does not compile: `cargo test -p tui-29er --no-run` fails with 4 E0063 errors in chat.rs:430, palette.rs:448/542, and tests.rs:45 because TuiSnapshot (connected_at, connecting_since, spinner_tick, last_read_message_id) and ChannelListItem (tier) gained fields not updated in test builders. Verified independently.","NMP architecture violations: TUI owns projection composition and derives app state (derive_channel_tree, unread tiers, member admin) in app.rs:173 instead of rendering NMP typed snapshots (nmp.29er.group_tree / nmp.nip29.*).","Relay selection/default/fallback policy is shell-owned and hardcoded (main.rs:39, login.rs:18, app.rs:307) rather than driven by Rust/NMP state and actions.","D8 violation: spawn_poller polls projections every 250ms (app.rs:196) instead of being driven by NMP update callbacks/snapshot frames.","Broken mentions feature: composer inserts @display_name (composer.rs:58), dispatch sends only content (app.rs:428), and mention tiers match @<my_pubkey> in previews (app.rs:671) — selected mention pubkeys are never dispatched.","Outbox confirmation matches by message content and accepts any pubkey when my_pubkey is missing (app.rs:338), so it can confirm the wrong item or never confirm.","Retained GroupChatProjection observers leak: every visited channel keeps an observer in self.chats with no close/unregister (app.rs:397); relay status is inferred from 'ever seen data' so empty relays stay Connecting and disconnects stay Connected (app.rs:187)."],"branch":"feat/tui-epic-full"}
