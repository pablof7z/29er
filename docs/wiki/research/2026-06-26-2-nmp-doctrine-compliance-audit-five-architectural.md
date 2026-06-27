---
type: research-record
date: 2026-06-26
session: 00700b1c-4fd7-4a41-90e9-7a76d64b42cc
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-29er/00700b1c-4fd7-4a41-90e9-7a76d64b42cc.jsonl
source_lines: 2145-2194
agent_attribution: main
has_preregistered_criteria: true
has_method: true
has_structured_report: true
characterization: "NMP doctrine compliance audit: five architectural checks (TUI/composer/publish_group_event/vendoring/tests) verified as passing"
captured_at: 2026-06-26T10:56:45Z
---

NMP doctrine compliance audit: five architectural checks (TUI/composer/publish_group_event/vendoring/tests) verified as passing

---

Assistant: Migration landed clean. Let me independently verify the key doctrine claims before calling it done:

User: [Tool result]: chpwd:5: command not found: _iterm2_set_profile
Your branch is up to date with 'origin/feat/nmp-v08-publish-group-event'.
Already up to date.
=== TUI free of NIP-21 knowledge? ===
crates/29er-tui/src/ui/composer.rs:64:        // rewrite (`@<hex>` → `nostr:npub1…`) at send time; the TUI holds zero
  ^ FOUND (bad)

=== shared composer exists in nmp-app-29er? ===
crates/nmp-app-29er/src/lib.rs
crates/nmp-app-29er/src/compose.rs
crates/nmp-app-29er/src/ffi.rs

=== publish_group_event used, old action gone? ===
crates/nmp-app-29er/src/ffi.rs:364:    fn chat_send_doorway_composes_kind9_publish_group_event() {
crates/nmp-app-29er/src/ffi.rs:643:    // `nmp.nip29.publish_group_event` action namespace. NIP-29 injects the
crates/nmp-app-29er/src/ffi.rs:703:const PUBLISH_GROUP_EVENT_NAMESPACE: &str = "nmp.nip29.publish_group_event";
crates/nmp-app-29er/src/ffi.rs:699:const CHAT_SEND_NAMESPACE: &str = "nmp.nip29.post_chat_message";
  ^ stale action ref (bad)

=== no vendored nmp? ===
  no vend

Assistant: Two flags to check. The TUI one is just a comment (documentation), but the `CHAT_SEND_NAMESPACE = "nmp.nip29.post_chat_message"` constant is concerning — that action no longer exists in v0.8.0, so any dispatch to it would be rejected at runtime even though it compiles. Let me inspect:

User: [Tool result]: 638	
639	    // The 29er chat-send doorway is the one place that composes. The shell
640	    // hands raw text + `@mention` pubkeys under `CHAT_SEND_NAMESPACE`; we run
641	    // the shared `compose_chat_message` (NIP-21 rewrite + `p` tags), wrap the
642	    // result as a kind:9 `PublishGroupEventInput`, and emit it under the real
643	    // `nmp.nip29.publish_group_event` action namespace. NIP-29 injects the
644	    // `h`/`previous` envelope itself — we must not (it rejects caller-supplied
645	    // envelope tags). Every other namespace re-encodes its body verbatim.
646	    let (dispatch_ns, payload): (&str, Vec<u8>) = if ns == CHAT_SEND_NAMESPACE {
647	        match encode_chat_send_payload(&body) {
648	            Some(p) => (PUBLISH_GROUP_EVENT_NAMESPACE, p),
649	            None => {
650	                return CString::new(
651	                    r#"{"error":"could not compose chat message from body"}"#,
652	                )
653	                .unwrap_or_else(|_| c"{}".to_

Assistant: That's actually correct by design — not a bug. `CHAT_SEND_NAMESPACE` is a **29er-internal doorway key**: the shell sends raw `{group, content, mention_pubkeys}` to it, and the FFI (line 646-648) composes via `compose_chat_message` then dispatches under the real `nmp.nip29.publish_group_event`. The TUI match was just a comment. All five doctrine checks pass:

- ✅ TUI holds zero NIP-21 code (only a comment)
- ✅ Shared `compose.rs` in `nmp-app-29er`
- ✅ Real dispatch target is `publish_group_event`; shell never sends envelope tags
- ✅ No vendored nmp
- ✅ 66 tests pass, workspace clean
