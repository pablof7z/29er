use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

use nmp_core::substrate::{BoundedMessageMap, KernelEvent, MAX_PROJECTION_MESSAGES};
use nmp_core::ObservedProjectionSink;
use nmp_nip29::kinds::{h_tag_value, KIND_CHAT_MESSAGE};
use nmp_nip29::projection::{DiscoveredGroup, DiscoveredGroupsSnapshot, JoinedGroupsSnapshot};

#[path = "wire/generated/group_tree_generated.rs"]
mod generated;

use generated::nmp_app_29er as fb;

pub const GROUP_TREE_SCHEMA_ID: &str = "nmp.29er.group_tree";
pub const GROUP_TREE_SCHEMA_VERSION: u32 = 1;
pub const GROUP_TREE_FILE_IDENTIFIER: &[u8; 4] = b"N29T";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupTreeMessageSummary {
    pub id: String,
    pub pubkey: String,
    pub preview: String,
    pub created_at: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GroupTreeMessageState {
    direct_unread_by_group: BTreeMap<String, u32>,
    last_message_by_group: BTreeMap<String, GroupTreeMessageSummary>,
}

impl GroupTreeMessageState {
    /// Direct unread count for a group's `local_id` (0 when unknown).
    #[must_use]
    pub fn unread_for(&self, group_id: &str) -> u32 {
        self.direct_unread_by_group.get(group_id).copied().unwrap_or(0)
    }

    /// Newest message summary for a group's `local_id`, if any.
    #[must_use]
    pub fn last_message_for(&self, group_id: &str) -> Option<&GroupTreeMessageSummary> {
        self.last_message_by_group.get(group_id)
    }
}

/// Per-group viewer membership truth for the active account.
///
/// Sourced from the account-scoped `JoinedGroupsProjection`, whose
/// `is_member`/`is_admin` are keyed on the active pubkey (relay-signed
/// kind:39001 / kind:39002 snapshots). The shell renders these flags; it does
/// not derive membership/admin from a roster scan (D11 — the kernel/app crate
/// owns membership truth).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GroupViewerMembership {
    pub is_member: bool,
    pub is_admin: bool,
}

/// Per-group membership lookup keyed by NIP-29 `local_id` (group id).
pub type GroupMembershipMap = BTreeMap<String, GroupViewerMembership>;

/// Build a per-group membership lookup from the account-scoped joined-groups
/// snapshot.
///
/// Returns an empty map (membership unknown / not surfaced) when `active_pubkey`
/// is empty or does not match the snapshot's `active_pubkey`. The mismatch guard
/// fails safe across an account switch: a `JoinedGroupsProjection` bakes its
/// pubkey at construction, so a snapshot whose `active_pubkey` no longer matches
/// the live active account is stale and must not leak the previous account's
/// membership/admin truth onto the current viewer. Reopening discovery rebuilds
/// the projection for the new account, at which point membership repopulates.
#[must_use]
pub fn membership_from_joined(
    joined: &JoinedGroupsSnapshot,
    active_pubkey: &str,
) -> GroupMembershipMap {
    if active_pubkey.is_empty() || joined.active_pubkey != active_pubkey {
        return GroupMembershipMap::new();
    }
    joined
        .groups
        .iter()
        .map(|group| {
            (
                group.group_id.clone(),
                GroupViewerMembership {
                    is_member: group.is_member,
                    is_admin: group.is_admin,
                },
            )
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StoredGroupTreeMessage {
    id: String,
    pubkey: String,
    preview: String,
    created_at: u64,
}

impl StoredGroupTreeMessage {
    fn from_event(event: &KernelEvent) -> Self {
        Self {
            id: event.id.clone(),
            pubkey: event.author.clone(),
            preview: event.content.trim().to_string(),
            created_at: event.created_at,
        }
    }

    fn into_summary(self) -> GroupTreeMessageSummary {
        GroupTreeMessageSummary {
            id: self.id,
            pubkey: self.pubkey,
            preview: self.preview,
            created_at: self.created_at,
        }
    }
}

#[derive(Debug)]
struct GroupMessageBucket {
    messages: BoundedMessageMap<String, StoredGroupTreeMessage>,
    unread_count: u32,
}

impl Default for GroupMessageBucket {
    fn default() -> Self {
        Self {
            messages: BoundedMessageMap::new(MAX_PROJECTION_MESSAGES),
            unread_count: 0,
        }
    }
}

/// Rust-owned read model for the group list's chat affordances.
///
/// It observes host-relay kind:9 traffic and exposes, per group, the newest
/// direct chat preview plus a direct unread count. The tree derivation folds
/// unread recursively so parents display their own unread plus descendants.
pub struct GroupTreeProjection {
    groups: Mutex<BTreeMap<String, GroupMessageBucket>>,
}

impl GroupTreeProjection {
    #[must_use]
    pub fn new() -> Self {
        Self {
            groups: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn mark_read(&self, group_id: &str) {
        let Ok(mut groups) = self.groups.lock() else {
            return;
        };
        let bucket = groups.entry(group_id.to_string()).or_default();
        bucket.unread_count = 0;
    }

    #[must_use]
    pub fn snapshot(&self) -> GroupTreeMessageState {
        let Ok(groups) = self.groups.lock() else {
            return GroupTreeMessageState::default();
        };

        let mut state = GroupTreeMessageState::default();
        for (group_id, bucket) in groups.iter() {
            if bucket.unread_count > 0 {
                state
                    .direct_unread_by_group
                    .insert(group_id.clone(), bucket.unread_count);
            }
            if let Some(last) = bucket.messages.values().cloned().max_by(|a, b| {
                a.created_at
                    .cmp(&b.created_at)
                    .then_with(|| a.id.cmp(&b.id))
            }) {
                state
                    .last_message_by_group
                    .insert(group_id.clone(), last.into_summary());
            }
        }
        state
    }
}

impl Default for GroupTreeProjection {
    fn default() -> Self {
        Self::new()
    }
}

impl ObservedProjectionSink for GroupTreeProjection {
    fn on_kernel_event(&self, event: &KernelEvent) {
        if event.kind != KIND_CHAT_MESSAGE {
            return;
        }
        let Some(group_id) = h_tag_value(&event.tags) else {
            return;
        };
        let Ok(mut groups) = self.groups.lock() else {
            return;
        };
        let bucket = groups.entry(group_id.to_string()).or_default();
        let was_new = bucket
            .messages
            .insert(event.id.clone(), StoredGroupTreeMessage::from_event(event))
            .is_none();
        if was_new {
            bucket.unread_count = bucket.unread_count.saturating_add(1);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct GroupTreeNode {
    group_id: String,
    host_relay_url: String,
    name: Option<String>,
    parent_id: Option<String>,
    child_ids: Vec<String>,
    member_count: u32,
    admin_count: u32,
    public: bool,
    open: bool,
    is_member: bool,
    is_admin: bool,
    last_message: Option<GroupTreeMessageSummary>,
    unread_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct GroupTreeSnapshot {
    host_relay_url: String,
    roots: Vec<GroupTreeNode>,
    nodes: Vec<GroupTreeNode>,
}

pub fn encode_group_tree_snapshot(
    discovered: &DiscoveredGroupsSnapshot,
    messages: &GroupTreeMessageState,
    membership: &GroupMembershipMap,
) -> Vec<u8> {
    let tree = derive_group_tree(discovered, messages, membership);
    let mut fbb = flatbuffers::FlatBufferBuilder::new();
    let root_offsets = encode_nodes(&mut fbb, &tree.roots);
    let node_offsets = encode_nodes(&mut fbb, &tree.nodes);
    let roots = fbb.create_vector(&root_offsets);
    let nodes = fbb.create_vector(&node_offsets);
    let host_relay_url = fbb.create_string(&tree.host_relay_url);
    let total_count = u32::try_from(tree.nodes.len()).unwrap_or(u32::MAX);

    let root = fb::GroupTreeSnapshot::create(
        &mut fbb,
        &fb::GroupTreeSnapshotArgs {
            schema_version: GROUP_TREE_SCHEMA_VERSION,
            host_relay_url: Some(host_relay_url),
            roots: Some(roots),
            nodes: Some(nodes),
            total_count,
        },
    );
    fb::finish_group_tree_snapshot_buffer(&mut fbb, root);
    fbb.finished_data().to_vec()
}

fn encode_nodes<'a>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'a>,
    nodes: &[GroupTreeNode],
) -> Vec<flatbuffers::WIPOffset<fb::GroupTreeNode<'a>>> {
    nodes.iter().map(|node| encode_node(fbb, node)).collect()
}

fn encode_node<'a>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'a>,
    node: &GroupTreeNode,
) -> flatbuffers::WIPOffset<fb::GroupTreeNode<'a>> {
    let group_id = fbb.create_string(&node.group_id);
    let host_relay_url = fbb.create_string(&node.host_relay_url);
    let name = node.name.as_deref().map(|value| fbb.create_string(value));
    let parent_id = node
        .parent_id
        .as_deref()
        .map(|value| fbb.create_string(value));
    let child_offsets: Vec<_> = node
        .child_ids
        .iter()
        .map(|child_id| fbb.create_string(child_id))
        .collect();
    let child_ids = fbb.create_vector(&child_offsets);
    let last_message_id = node
        .last_message
        .as_ref()
        .map(|message| fbb.create_string(&message.id));
    let last_message_pubkey = node
        .last_message
        .as_ref()
        .map(|message| fbb.create_string(&message.pubkey));
    let last_message_preview = node
        .last_message
        .as_ref()
        .map(|message| fbb.create_string(&message.preview));
    let last_message_created_at = node
        .last_message
        .as_ref()
        .map(|message| message.created_at)
        .unwrap_or_default();

    fb::GroupTreeNode::create(
        fbb,
        &fb::GroupTreeNodeArgs {
            group_id: Some(group_id),
            host_relay_url: Some(host_relay_url),
            name,
            parent_id,
            child_ids: Some(child_ids),
            member_count: node.member_count,
            admin_count: node.admin_count,
            public: node.public,
            open: node.open,
            is_member: node.is_member,
            is_admin: node.is_admin,
            branch: !node.child_ids.is_empty(),
            last_message_id,
            last_message_pubkey,
            last_message_preview,
            last_message_created_at,
            unread_count: node.unread_count,
        },
    )
}

fn derive_group_tree(
    discovered: &DiscoveredGroupsSnapshot,
    messages: &GroupTreeMessageState,
    membership: &GroupMembershipMap,
) -> GroupTreeSnapshot {
    let groups_by_id: BTreeMap<_, _> = discovered
        .groups
        .iter()
        .map(|group| (group.group_id.as_str(), group))
        .collect();
    let known_ids: BTreeSet<_> = groups_by_id.keys().copied().collect();
    let mut parent_by_child: BTreeMap<&str, &str> = BTreeMap::new();
    let mut children_by_parent: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();

    for group in &discovered.groups {
        if let Some(parent) = group.parent.as_deref() {
            if parent != group.group_id && known_ids.contains(parent) {
                parent_by_child.insert(&group.group_id, parent);
                children_by_parent
                    .entry(parent)
                    .or_default()
                    .insert(&group.group_id);
            }
        }
        for child in &group.children {
            let child = child.as_str();
            if child != group.group_id && known_ids.contains(child) {
                children_by_parent
                    .entry(&group.group_id)
                    .or_default()
                    .insert(child);
                parent_by_child.entry(child).or_insert(&group.group_id);
            }
        }
    }

    let mut nodes_by_id: BTreeMap<String, GroupTreeNode> = BTreeMap::new();
    for group in &discovered.groups {
        let parent_id = parent_by_child
            .get(group.group_id.as_str())
            .map(|value| (*value).to_string());
        let node = build_node(group, parent_id, &children_by_parent, messages, membership);
        nodes_by_id.insert(node.group_id.clone(), node);
    }

    let root_ids: Vec<String> = nodes_by_id
        .values()
        .filter(|node| node.parent_id.is_none())
        .map(|node| node.group_id.clone())
        .collect();

    let mut aggregate_cache = BTreeMap::new();
    for id in nodes_by_id.keys().cloned().collect::<Vec<_>>() {
        let count = aggregate_unread_count(&id, &nodes_by_id, &mut aggregate_cache);
        if let Some(node) = nodes_by_id.get_mut(&id) {
            node.unread_count = count;
        }
    }

    let roots = root_ids
        .iter()
        .filter_map(|id| nodes_by_id.get(id).cloned())
        .collect();
    let nodes = discovered
        .groups
        .iter()
        .filter_map(|group| nodes_by_id.get(&group.group_id).cloned())
        .collect();

    GroupTreeSnapshot {
        host_relay_url: discovered.host_relay_url.clone(),
        roots,
        nodes,
    }
}

fn build_node(
    group: &DiscoveredGroup,
    parent_id: Option<String>,
    children_by_parent: &BTreeMap<&str, BTreeSet<&str>>,
    messages: &GroupTreeMessageState,
    membership: &GroupMembershipMap,
) -> GroupTreeNode {
    let child_ids = children_by_parent
        .get(group.group_id.as_str())
        .map(|children| children.iter().map(|child| (*child).to_string()).collect())
        .unwrap_or_default();
    let viewer = membership
        .get(&group.group_id)
        .copied()
        .unwrap_or_default();
    GroupTreeNode {
        group_id: group.group_id.clone(),
        host_relay_url: group.host_relay_url.clone(),
        name: group.name.clone(),
        parent_id,
        child_ids,
        member_count: group.member_count,
        admin_count: group.admin_count,
        public: group.public,
        open: group.open,
        is_member: viewer.is_member,
        is_admin: viewer.is_admin,
        last_message: messages.last_message_by_group.get(&group.group_id).cloned(),
        unread_count: messages
            .direct_unread_by_group
            .get(&group.group_id)
            .copied()
            .unwrap_or_default(),
    }
}

fn aggregate_unread_count(
    group_id: &str,
    nodes_by_id: &BTreeMap<String, GroupTreeNode>,
    cache: &mut BTreeMap<String, u32>,
) -> u32 {
    if let Some(count) = cache.get(group_id) {
        return *count;
    }
    let Some(node) = nodes_by_id.get(group_id) else {
        return 0;
    };
    let count = node
        .child_ids
        .iter()
        .fold(node.unread_count, |acc, child_id| {
            acc.saturating_add(aggregate_unread_count(child_id, nodes_by_id, cache))
        });
    cache.insert(group_id.to_string(), count);
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn discovered() -> DiscoveredGroupsSnapshot {
        DiscoveredGroupsSnapshot {
            host_relay_url: "wss://groups.example.com".to_string(),
            groups: vec![
                group("root", None, ["child"]),
                group("child", Some("root"), []),
                group("sibling", None, []),
            ],
        }
    }

    fn group<const N: usize>(
        id: &str,
        parent: Option<&str>,
        children: [&str; N],
    ) -> DiscoveredGroup {
        DiscoveredGroup {
            group_id: id.to_string(),
            host_relay_url: "wss://groups.example.com".to_string(),
            name: Some(id.to_string()),
            picture: None,
            about: None,
            member_count: 0,
            admin_count: 0,
            public: true,
            open: true,
            parent: parent.map(str::to_string),
            children: children.into_iter().map(str::to_string).collect(),
        }
    }

    fn event(id: &str, group_id: &str, created_at: u64, content: &str) -> KernelEvent {
        KernelEvent {
            id: id.to_string(),
            author: "pubkey".to_string(),
            kind: KIND_CHAT_MESSAGE,
            created_at,
            tags: vec![vec!["h".to_string(), group_id.to_string()]],
            content: content.to_string(),
            relay_provenance: Vec::new(),
        }
    }

    #[test]
    fn tree_rows_include_direct_last_kind9_preview() {
        let projection = GroupTreeProjection::new();
        projection.on_kernel_event(&event("old", "child", 10, "older"));
        projection.on_kernel_event(&event("new", "child", 20, "newer"));

        let tree = derive_group_tree(&discovered(), &projection.snapshot(), &GroupMembershipMap::new());
        let child = tree
            .nodes
            .iter()
            .find(|node| node.group_id == "child")
            .expect("child node");

        assert_eq!(
            child
                .last_message
                .as_ref()
                .map(|message| message.preview.as_str()),
            Some("newer")
        );
        assert_eq!(
            child
                .last_message
                .as_ref()
                .map(|message| message.created_at),
            Some(20)
        );
    }

    #[test]
    fn unread_count_aggregates_group_and_descendants() {
        let projection = GroupTreeProjection::new();
        projection.on_kernel_event(&event("root-msg", "root", 10, "root direct"));
        projection.on_kernel_event(&event("child-msg", "child", 20, "child direct"));

        let tree = derive_group_tree(&discovered(), &projection.snapshot(), &GroupMembershipMap::new());
        let root = tree
            .nodes
            .iter()
            .find(|node| node.group_id == "root")
            .expect("root node");
        let child = tree
            .nodes
            .iter()
            .find(|node| node.group_id == "child")
            .expect("child node");

        assert_eq!(root.unread_count, 2);
        assert_eq!(child.unread_count, 1);
    }

    #[test]
    fn marking_child_read_updates_parent_aggregate_unread() {
        let projection = GroupTreeProjection::new();
        projection.on_kernel_event(&event("root-msg", "root", 10, "root direct"));
        projection.on_kernel_event(&event("child-msg", "child", 20, "child direct"));
        projection.mark_read("child");

        let tree = derive_group_tree(&discovered(), &projection.snapshot(), &GroupMembershipMap::new());
        let root = tree
            .nodes
            .iter()
            .find(|node| node.group_id == "root")
            .expect("root node");
        let child = tree
            .nodes
            .iter()
            .find(|node| node.group_id == "child")
            .expect("child node");

        assert_eq!(root.unread_count, 1);
        assert_eq!(child.unread_count, 0);
    }

    #[test]
    fn duplicate_events_do_not_increment_unread() {
        let projection = GroupTreeProjection::new();
        let event = event("same", "root", 10, "first");
        projection.on_kernel_event(&event);
        projection.on_kernel_event(&event);

        let tree = derive_group_tree(&discovered(), &projection.snapshot(), &GroupMembershipMap::new());
        let root = tree
            .nodes
            .iter()
            .find(|node| node.group_id == "root")
            .expect("root node");

        assert_eq!(root.unread_count, 1);
    }

    fn joined(active_pubkey: &str) -> JoinedGroupsSnapshot {
        use nmp_nip29::projection::JoinedGroup;
        JoinedGroupsSnapshot {
            active_pubkey: active_pubkey.to_string(),
            groups: vec![
                JoinedGroup {
                    group_id: "root".to_string(),
                    host_relay_url: "wss://groups.example.com".to_string(),
                    is_member: true,
                    is_admin: true,
                    ..Default::default()
                },
                JoinedGroup {
                    group_id: "child".to_string(),
                    host_relay_url: "wss://groups.example.com".to_string(),
                    is_member: true,
                    is_admin: false,
                    ..Default::default()
                },
            ],
        }
    }

    #[test]
    fn nodes_carry_viewer_membership_from_joined_projection() {
        let membership = membership_from_joined(&joined("viewer-pubkey"), "viewer-pubkey");
        let tree = derive_group_tree(
            &discovered(),
            &GroupTreeMessageState::default(),
            &membership,
        );

        let root = tree
            .nodes
            .iter()
            .find(|node| node.group_id == "root")
            .expect("root node");
        let child = tree
            .nodes
            .iter()
            .find(|node| node.group_id == "child")
            .expect("child node");
        let sibling = tree
            .nodes
            .iter()
            .find(|node| node.group_id == "sibling")
            .expect("sibling node");

        assert!(root.is_member && root.is_admin);
        assert!(child.is_member && !child.is_admin);
        // No joined row → membership defaults to false/false.
        assert!(!sibling.is_member && !sibling.is_admin);
    }

    #[test]
    fn membership_is_empty_when_active_pubkey_mismatches_snapshot() {
        // Snapshot baked for a different account must not leak membership onto
        // the current viewer (fail-safe across an account switch).
        let membership = membership_from_joined(&joined("other-pubkey"), "viewer-pubkey");
        assert!(membership.is_empty());

        // An empty active pubkey (no signed-in account) is also empty.
        let none = membership_from_joined(&joined(""), "");
        assert!(none.is_empty());
    }
}
