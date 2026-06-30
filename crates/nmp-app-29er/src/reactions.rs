//! 29er reaction convenience methods.
//!
//! Reactions are NIP-25 artifacts. This module builds the NIP-25 kind:7 event
//! through `nmp-nip25`, then transports that event through NIP-29's generic
//! group-publish action.

use nmp_nip29::GroupId;

use crate::{DispatchOutcome, TwentyNinerApp};

#[uniffi::export]
impl TwentyNinerApp {
    pub fn react_to_group_message(
        &self,
        group_id_json: String,
        event_id: String,
        event_author_pubkey: Option<String>,
        reaction: String,
    ) -> DispatchOutcome {
        let Some(body_json) =
            reaction_group_publish_body(&group_id_json, event_id, event_author_pubkey, reaction)
        else {
            return DispatchOutcome::error("could not compose NIP-25 group reaction");
        };
        crate::dispatch::dispatch_nip29_action(
            self.app(),
            "nmp.nip29.publish_group_event",
            &body_json,
        )
    }
}

fn reaction_group_publish_body(
    group_id_json: &str,
    event_id: String,
    event_author_pubkey: Option<String>,
    reaction: String,
) -> Option<String> {
    let group = serde_json::from_str::<GroupId>(group_id_json).ok()?;
    let target_author_pubkey = event_author_pubkey.and_then(non_empty_trimmed);
    let reaction = non_empty_trimmed(reaction).unwrap_or_else(|| "+".to_string());
    let event = nmp_nip25::build_reaction_event(&nmp_nip25::ReactAction {
        target_event_id: event_id,
        reaction,
        target_author_pubkey,
    })
    .ok()?;
    serde_json::to_string(&nmp_nip29::action::PublishGroupEventInput {
        group,
        kind: event.kind,
        content: event.content,
        tags: event.tags,
    })
    .ok()
}

fn non_empty_trimmed(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EVENT_ID: &str = "1111111111111111111111111111111111111111111111111111111111111111";
    const AUTHOR: &str = "2222222222222222222222222222222222222222222222222222222222222222";

    fn group_id_json() -> String {
        r#"{"host_relay_url":"wss://groups.example.com","local_id":"room"}"#.to_string()
    }

    #[test]
    fn reaction_body_builds_kind7_group_publish_payload() {
        let body = reaction_group_publish_body(
            &group_id_json(),
            EVENT_ID.to_string(),
            Some(AUTHOR.to_string()),
            "+".to_string(),
        )
        .expect("reaction body composes");
        let input: nmp_nip29::action::PublishGroupEventInput =
            serde_json::from_str(&body).expect("publish input json");
        assert_eq!(input.group.local_id, "room");
        assert_eq!(input.kind, nmp_nip25::KIND_REACTION);
        assert_eq!(input.content, "+");
        assert!(input
            .tags
            .iter()
            .any(|tag| tag == &vec!["e".to_string(), EVENT_ID.to_string()]));
        assert!(input
            .tags
            .iter()
            .any(|tag| tag == &vec!["p".to_string(), AUTHOR.to_string()]));
    }

    #[test]
    fn reaction_body_fails_closed_on_bad_inputs() {
        assert!(reaction_group_publish_body(
            "not json",
            EVENT_ID.to_string(),
            None,
            "+".to_string()
        )
        .is_none());
        assert!(reaction_group_publish_body(
            &group_id_json(),
            "not-event-id".to_string(),
            None,
            "+".to_string(),
        )
        .is_none());
    }
}
