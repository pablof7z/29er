use super::*;

const AUTHOR: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const OTHER: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const EVENT_ID: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

fn event(id: &str, created_at: u64, kind: u32, content: &str) -> GroupEvent {
    GroupEvent {
        id: id.to_string(),
        pubkey: AUTHOR.to_string(),
        content: content.to_string(),
        created_at,
        kind,
    }
}

fn snapshot(events: Vec<GroupEvent>) -> GroupEventsSnapshot {
    GroupEventsSnapshot { events }
}

#[test]
fn plain_text_message_keeps_raw_copy_and_content_tree_bytes() {
    let chat = derive_group_chat_snapshot(&snapshot(vec![event("e1", 20, 9, "hello")]));

    assert_eq!(chat.messages.len(), 1);
    let message = &chat.messages[0];
    assert_eq!(message.raw_content, "hello");
    assert_eq!(message.copy_text, "hello");
    assert!(!message.content_tree_bytes.is_empty());
    assert!(message.mention_pubkeys.is_empty());
    assert!(message.event_ref_uris.is_empty());
    assert_eq!(chat.profile_demand_pubkeys, vec![AUTHOR.to_string()]);
}

#[test]
fn profile_mentions_are_message_and_top_level_demands() {
    let npub = nmp_nostr_id::encode_npub(OTHER).expect("fixture npub encodes");
    let chat = derive_group_chat_snapshot(&snapshot(vec![event(
        "e1",
        20,
        9,
        &format!("hello @{npub}"),
    )]));

    let message = &chat.messages[0];
    assert_eq!(message.mention_pubkeys, vec![OTHER.to_string()]);
    assert_eq!(
        chat.profile_demand_pubkeys,
        vec![AUTHOR.to_string(), OTHER.to_string()]
    );
}

#[test]
fn event_refs_are_message_and_top_level_demands() {
    let note = nmp_nostr_id::encode_note(EVENT_ID).expect("fixture note encodes");
    let chat = derive_group_chat_snapshot(&snapshot(vec![event(
        "e1",
        20,
        9,
        &format!("see nostr:{note}"),
    )]));

    let message = &chat.messages[0];
    assert_eq!(message.event_ref_primary_ids, vec![EVENT_ID.to_string()]);
    assert_eq!(chat.event_ref_primary_ids, vec![EVENT_ID.to_string()]);
    assert_eq!(message.event_ref_uris, vec![format!("nostr:{note}")]);
    assert_eq!(chat.event_ref_uris, vec![format!("nostr:{note}")]);
}

#[test]
fn malformed_event_ref_falls_back_to_raw_text_without_demands() {
    let chat =
        derive_group_chat_snapshot(&snapshot(vec![event("e1", 20, 9, "see nostr:notvalid")]));

    let message = &chat.messages[0];
    assert_eq!(message.raw_content, "see nostr:notvalid");
    assert_eq!(message.copy_text, "see nostr:notvalid");
    assert!(message.event_ref_primary_ids.is_empty());
    assert!(message.event_ref_uris.is_empty());
    assert!(chat.event_ref_primary_ids.is_empty());
    assert!(chat.event_ref_uris.is_empty());
}

#[test]
fn projection_preserves_raw_newest_first_order_and_clear_updates() {
    let full = derive_group_chat_snapshot(&snapshot(vec![
        event("new", 30, 9, "new"),
        event("old", 10, 9, "old"),
    ]));
    assert_eq!(
        full.messages
            .iter()
            .map(|m| m.id.as_str())
            .collect::<Vec<_>>(),
        vec!["new", "old"]
    );

    let empty = derive_group_chat_snapshot(&snapshot(Vec::new()));
    assert!(empty.messages.is_empty());
    assert!(empty.profile_demand_pubkeys.is_empty());
    assert!(empty.event_ref_primary_ids.is_empty());
}

#[test]
fn flatbuffer_encoding_uses_app_owned_schema_and_nfct_tree_bytes() {
    let bytes = encode_group_chat_snapshot(&snapshot(vec![event("e1", 20, 9, "hello")]));
    assert!(flatbuffers::buffer_has_identifier(&bytes, "N29C", false));
    let reader = flatbuffers::root::<fb::GroupChatSnapshot>(&bytes).expect("N29C buffer decodes");
    assert_eq!(reader.schema_version(), GROUP_CHAT_SCHEMA_VERSION);
    let message = reader.messages().expect("messages").get(0);
    assert_eq!(message.id(), "e1");
    let tree_bytes = message.content_tree_bytes().expect("tree bytes");
    assert!(flatbuffers::buffer_has_identifier(
        tree_bytes.bytes(),
        "NFCT",
        false
    ));
}
