use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use nmp_content::wire::encode_content_tree;
use nmp_content::{RenderMode, WireNode, WireNostrUriKind};
use nmp_nip25::{
    ReactionAggregateProjection, ReactionAggregateSnapshot, ReactionEmojiCount,
    ReactionTargetAggregate,
};
use nmp_nip29::projection::{GroupEvent, GroupEventsProjection, GroupEventsSnapshot};

#[path = "wire/generated/group_chat_generated.rs"]
mod generated;

use generated::nmp_app_29er as fb;

pub const GROUP_CHAT_SCHEMA_ID: &str = "app.29er.group_chat";
pub const GROUP_CHAT_SCHEMA_VERSION: u32 = 2;
pub const GROUP_CHAT_FILE_IDENTIFIER: &[u8; 4] = b"N29C";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GroupChatReaction {
    pub emoji: String,
    pub count: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GroupChatMessage {
    pub id: String,
    pub pubkey: String,
    pub raw_content: String,
    pub copy_text: String,
    pub created_at: u64,
    pub kind: u32,
    pub content_tree_bytes: Vec<u8>,
    pub mention_pubkeys: Vec<String>,
    pub event_ref_uris: Vec<String>,
    pub event_ref_primary_ids: Vec<String>,
    pub reactions: Vec<GroupChatReaction>,
    pub reaction_reactor_pubkeys: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GroupChatSnapshot {
    pub messages: Vec<GroupChatMessage>,
    pub profile_demand_pubkeys: Vec<String>,
    pub event_ref_uris: Vec<String>,
    pub event_ref_primary_ids: Vec<String>,
}

/// 29er-owned chat read model layered over NMP's raw NIP-29 group-events
/// substrate.
///
/// The raw reader remains the ingest/session substrate. Consumers read this
/// app projection, which owns product-specific enrichment: content trees,
/// mention/event-ref demand lists, copy text, and message fields shaped for
/// the group-chat surface.
pub struct GroupChatProjection {
    raw: Arc<GroupEventsProjection>,
    reactions: Arc<ReactionAggregateProjection>,
}

impl GroupChatProjection {
    #[must_use]
    pub fn new(raw: Arc<GroupEventsProjection>) -> Self {
        Self::new_with_reactions(raw, Arc::new(ReactionAggregateProjection::default()))
    }

    #[must_use]
    pub fn new_with_reactions(
        raw: Arc<GroupEventsProjection>,
        reactions: Arc<ReactionAggregateProjection>,
    ) -> Self {
        Self { raw, reactions }
    }

    #[must_use]
    pub fn snapshot(&self) -> GroupChatSnapshot {
        derive_group_chat_snapshot_with_reactions(&self.raw.snapshot(), &self.reactions.snapshot())
    }
}

#[must_use]
pub fn derive_group_chat_snapshot(raw: &GroupEventsSnapshot) -> GroupChatSnapshot {
    derive_group_chat_snapshot_with_reactions(raw, &ReactionAggregateSnapshot::empty())
}

#[must_use]
pub fn derive_group_chat_snapshot_with_reactions(
    raw: &GroupEventsSnapshot,
    reactions: &ReactionAggregateSnapshot,
) -> GroupChatSnapshot {
    let mut profile_demand_pubkeys = BTreeSet::new();
    let mut snapshot_event_ref_uris = BTreeSet::new();
    let mut snapshot_event_ref_primary_ids = BTreeSet::new();
    let reactions_by_target = reactions_by_target(reactions);

    let messages = raw
        .events
        .iter()
        .map(|event| {
            let message = derive_group_chat_message(
                event,
                &mut profile_demand_pubkeys,
                &mut snapshot_event_ref_uris,
                &mut snapshot_event_ref_primary_ids,
                reactions_by_target.get(event.id.as_str()).copied(),
            );
            profile_demand_pubkeys.insert(message.pubkey.clone());
            profile_demand_pubkeys.extend(message.reaction_reactor_pubkeys.iter().cloned());
            message
        })
        .collect();

    GroupChatSnapshot {
        messages,
        profile_demand_pubkeys: profile_demand_pubkeys.into_iter().collect(),
        event_ref_uris: snapshot_event_ref_uris.into_iter().collect(),
        event_ref_primary_ids: snapshot_event_ref_primary_ids.into_iter().collect(),
    }
}

#[must_use]
pub fn encode_group_chat_snapshot(raw: &GroupEventsSnapshot) -> Vec<u8> {
    let snapshot = derive_group_chat_snapshot(raw);
    encode_group_chat_projection(&snapshot)
}

#[must_use]
pub fn encode_group_chat_snapshot_with_reactions(
    raw: &GroupEventsSnapshot,
    reactions: &ReactionAggregateSnapshot,
) -> Vec<u8> {
    let snapshot = derive_group_chat_snapshot_with_reactions(raw, reactions);
    encode_group_chat_projection(&snapshot)
}

fn derive_group_chat_message(
    event: &GroupEvent,
    profile_demand_pubkeys: &mut BTreeSet<String>,
    snapshot_event_ref_uris: &mut BTreeSet<String>,
    snapshot_event_ref_primary_ids: &mut BTreeSet<String>,
    reactions: Option<&ReactionTargetAggregate>,
) -> GroupChatMessage {
    let content_tree =
        nmp_content::tokenize_with_kind(&event.content, &[], RenderMode::Auto, event.kind)
            .to_wire();
    let content_tree_bytes = encode_content_tree(&content_tree);
    let mut mention_pubkeys = BTreeSet::new();
    let mut event_ref_uris = BTreeSet::new();
    let mut event_ref_primary_ids = BTreeSet::new();

    for node in &content_tree.nodes {
        match node {
            WireNode::Mention { uri } => {
                if uri.kind == WireNostrUriKind::Profile && is_hex_id_64(&uri.primary_id) {
                    mention_pubkeys.insert(uri.primary_id.clone());
                    profile_demand_pubkeys.insert(uri.primary_id.clone());
                }
            }
            WireNode::EventRef { uri } => {
                if matches!(
                    uri.kind,
                    WireNostrUriKind::Event | WireNostrUriKind::Address
                ) {
                    if !uri.uri.is_empty() {
                        event_ref_uris.insert(uri.uri.clone());
                        snapshot_event_ref_uris.insert(uri.uri.clone());
                    }
                    if !uri.primary_id.is_empty() {
                        event_ref_primary_ids.insert(uri.primary_id.clone());
                        snapshot_event_ref_primary_ids.insert(uri.primary_id.clone());
                    }
                    if let Some(author) = uri.author.as_ref().filter(|value| is_hex_id_64(value)) {
                        profile_demand_pubkeys.insert(author.clone());
                    }
                }
            }
            _ => {}
        }
    }

    let (reactions, reaction_reactor_pubkeys) = reactions
        .map(|aggregate| {
            (
                aggregate
                    .by_emoji
                    .iter()
                    .map(GroupChatReaction::from)
                    .collect(),
                aggregate.reactors.clone(),
            )
        })
        .unwrap_or_default();

    GroupChatMessage {
        id: event.id.clone(),
        pubkey: event.pubkey.clone(),
        raw_content: event.content.clone(),
        copy_text: event.content.clone(),
        created_at: event.created_at,
        kind: event.kind,
        content_tree_bytes,
        mention_pubkeys: mention_pubkeys.into_iter().collect(),
        event_ref_uris: event_ref_uris.into_iter().collect(),
        event_ref_primary_ids: event_ref_primary_ids.into_iter().collect(),
        reactions,
        reaction_reactor_pubkeys,
    }
}

impl From<&ReactionEmojiCount> for GroupChatReaction {
    fn from(value: &ReactionEmojiCount) -> Self {
        Self {
            emoji: value.token.clone(),
            count: value.count,
        }
    }
}

fn reactions_by_target(
    reactions: &ReactionAggregateSnapshot,
) -> BTreeMap<&str, &ReactionTargetAggregate> {
    reactions
        .targets
        .iter()
        .map(|target| (target.target_event_id.as_str(), target))
        .collect()
}

fn encode_group_chat_projection(snapshot: &GroupChatSnapshot) -> Vec<u8> {
    let mut fbb = flatbuffers::FlatBufferBuilder::new();
    let message_offsets: Vec<_> = snapshot
        .messages
        .iter()
        .map(|message| encode_message(&mut fbb, message))
        .collect();
    let messages = fbb.create_vector(&message_offsets);
    let profile_demand_pubkeys = encode_string_vector(&mut fbb, &snapshot.profile_demand_pubkeys);
    let event_ref_uris = encode_string_vector(&mut fbb, &snapshot.event_ref_uris);
    let event_ref_primary_ids = encode_string_vector(&mut fbb, &snapshot.event_ref_primary_ids);

    let root = fb::GroupChatSnapshot::create(
        &mut fbb,
        &fb::GroupChatSnapshotArgs {
            schema_version: GROUP_CHAT_SCHEMA_VERSION,
            messages: Some(messages),
            profile_demand_pubkeys: Some(profile_demand_pubkeys),
            event_ref_uris: Some(event_ref_uris),
            event_ref_primary_ids: Some(event_ref_primary_ids),
        },
    );
    fb::finish_group_chat_snapshot_buffer(&mut fbb, root);
    fbb.finished_data().to_vec()
}

fn encode_message<'a>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'a>,
    message: &GroupChatMessage,
) -> flatbuffers::WIPOffset<fb::GroupChatMessage<'a>> {
    let id = fbb.create_string(&message.id);
    let pubkey = fbb.create_string(&message.pubkey);
    let raw_content = fbb.create_string(&message.raw_content);
    let copy_text = fbb.create_string(&message.copy_text);
    let content_tree_bytes = fbb.create_vector(&message.content_tree_bytes);
    let mention_pubkeys = encode_string_vector(fbb, &message.mention_pubkeys);
    let event_ref_uris = encode_string_vector(fbb, &message.event_ref_uris);
    let event_ref_primary_ids = encode_string_vector(fbb, &message.event_ref_primary_ids);
    let reaction_offsets: Vec<_> = message
        .reactions
        .iter()
        .map(|reaction| encode_reaction(fbb, reaction))
        .collect();
    let reactions = fbb.create_vector(&reaction_offsets);
    let reaction_reactor_pubkeys = encode_string_vector(fbb, &message.reaction_reactor_pubkeys);

    fb::GroupChatMessage::create(
        fbb,
        &fb::GroupChatMessageArgs {
            id: Some(id),
            pubkey: Some(pubkey),
            raw_content: Some(raw_content),
            copy_text: Some(copy_text),
            created_at: message.created_at,
            kind: message.kind,
            content_tree_bytes: Some(content_tree_bytes),
            mention_pubkeys: Some(mention_pubkeys),
            event_ref_uris: Some(event_ref_uris),
            event_ref_primary_ids: Some(event_ref_primary_ids),
            reactions: Some(reactions),
            reaction_reactor_pubkeys: Some(reaction_reactor_pubkeys),
        },
    )
}

fn encode_reaction<'a>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'a>,
    reaction: &GroupChatReaction,
) -> flatbuffers::WIPOffset<fb::GroupChatReaction<'a>> {
    let emoji = fbb.create_string(&reaction.emoji);
    fb::GroupChatReaction::create(
        fbb,
        &fb::GroupChatReactionArgs {
            emoji: Some(emoji),
            count: reaction.count,
        },
    )
}

fn encode_string_vector<'a>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'a>,
    values: &[String],
) -> flatbuffers::WIPOffset<flatbuffers::Vector<'a, flatbuffers::ForwardsUOffset<&'a str>>> {
    let offsets: Vec<_> = values
        .iter()
        .map(|value| fbb.create_string(value))
        .collect();
    fbb.create_vector(&offsets)
}

fn is_hex_id_64(value: &str) -> bool {
    value.len() == 64 && value.as_bytes().iter().all(u8::is_ascii_hexdigit)
}

#[cfg(test)]
mod tests;
