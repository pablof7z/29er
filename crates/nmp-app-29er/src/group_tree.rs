use std::collections::{BTreeMap, BTreeSet};

use nmp_nip29::projection::{DiscoveredGroup, DiscoveredGroupsSnapshot};

#[path = "wire/generated/group_tree_generated.rs"]
mod generated;

use generated::nmp_app_29er as fb;

pub const GROUP_TREE_SCHEMA_ID: &str = "nmp.29er.group_tree";
pub const GROUP_TREE_SCHEMA_VERSION: u32 = 1;
pub const GROUP_TREE_FILE_IDENTIFIER: &[u8; 4] = b"N29T";

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
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct GroupTreeSnapshot {
    host_relay_url: String,
    roots: Vec<GroupTreeNode>,
    nodes: Vec<GroupTreeNode>,
}

pub fn encode_group_tree_snapshot(discovered: &DiscoveredGroupsSnapshot) -> Vec<u8> {
    let tree = derive_group_tree(discovered);
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
    let parent_id = node.parent_id.as_deref().map(|value| fbb.create_string(value));
    let child_offsets: Vec<_> = node
        .child_ids
        .iter()
        .map(|child_id| fbb.create_string(child_id))
        .collect();
    let child_ids = fbb.create_vector(&child_offsets);

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
            branch: !node.child_ids.is_empty(),
        },
    )
}

fn derive_group_tree(discovered: &DiscoveredGroupsSnapshot) -> GroupTreeSnapshot {
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

    let mut nodes = Vec::with_capacity(discovered.groups.len());
    let mut roots = Vec::new();
    for group in &discovered.groups {
        let parent_id = parent_by_child
            .get(group.group_id.as_str())
            .map(|value| (*value).to_string());
        let node = build_node(group, parent_id, &children_by_parent);
        if node.parent_id.is_none() {
            roots.push(node.clone());
        }
        nodes.push(node);
    }

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
) -> GroupTreeNode {
    let child_ids = children_by_parent
        .get(group.group_id.as_str())
        .map(|children| children.iter().map(|child| (*child).to_string()).collect())
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
    }
}
