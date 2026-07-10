//! 29er group-list preview source sessions.
//!
//! NMP owns routing, observed-projection delivery, AND the per-group
//! membership reconcile: this module composes one relay-pinned kind:9 source
//! per discovered NIP-29 group into 29er's app-owned group-tree preview
//! projection over an `nmp_read_session::KeyedReadCollection` (#3115 shape
//! (b) — the raw-observed-projection flavor), instead of hand-diffing a
//! `BTreeMap` of open sessions itself.
//!
//! [`GroupPreviewSessions::sync`] is a pure `reconcile()` call — no lock is
//! held across the host `open_observed_projection` call (see
//! `KeyedReadCollection`'s own module docs) — so it is safe to drive from any
//! lane. Callers MUST still only call it from the read/actor lane, once per
//! discovered-groups change, never from inside a snapshot-tick closure (the
//! 29er#60 deadlock class; see `group_sessions.rs`).

use std::collections::BTreeMap;
use std::sync::Arc;

use nmp_core::substrate::{ObservedProjection, ObservedProjectionRegistrar};
use nmp_core::ObservedProjectionSink;
use nmp_native_runtime::NmpApp;
use nmp_nip29::projection::DiscoveredGroupsSnapshot;
use nmp_nip29::GroupId;
use nmp_planner::InterestShape;
use nmp_read_session::{KeyedReadCollection, MemberKey, TeardownAction};

use crate::group_tree::GroupTreeProjection;
use crate::kinds::KIND_CHAT_MESSAGE;

const GROUP_PREVIEW_REPLAY_LIMIT: usize = 80;
const GROUP_PREVIEW_SCOPE_GLOBAL: u32 = 1;

/// Live per-group kind:9 preview sources, reconciled through a
/// [`KeyedReadCollection`] keyed by the full `GroupId` (host relay + local
/// id — the injective identity `KeyedReadCollection`'s `key_fn` contract
/// requires, per its module docs).
pub struct GroupPreviewSessions {
    collection: KeyedReadCollection<GroupId, GroupId>,
}

impl GroupPreviewSessions {
    /// Builds the collection. `projection` is the SHARED multi-group sink
    /// every per-group observed projection feeds (unlike collection B's
    /// presence sessions, one `GroupTreeProjection` fans in every group's
    /// kind:9 traffic by tag, so the same `Arc` is reused across all keys).
    #[must_use]
    pub fn new(app: Arc<NmpApp>, projection: Arc<GroupTreeProjection>) -> Self {
        let collection = KeyedReadCollection::new(
            "29er.group-tree.preview",
            member_key,
            move |_member_key, group: GroupId| {
                open_group_preview(Arc::clone(&app), Arc::clone(&projection), group)
            },
        );
        Self { collection }
    }

    /// Reconciles live preview sources to exactly the discovered group set.
    /// Mounts a source for every newly-discovered group and withdraws one
    /// for every group no longer discovered (including its accumulated
    /// preview state — see [`open_group_preview`]'s teardown).
    pub fn sync(&self, discovered: &DiscoveredGroupsSnapshot) {
        let desired: BTreeMap<GroupId, GroupId> = discovered
            .groups
            .iter()
            .filter(|group| !group.group_id.is_empty() && !group.host_relay_url.is_empty())
            .map(|group| {
                let id = GroupId::new(group.host_relay_url.clone(), group.group_id.clone());
                (id.clone(), id)
            })
            .collect();
        self.collection.reconcile(desired);
    }

    pub fn close_all(&self, projection: &GroupTreeProjection) {
        self.collection.close();
        projection.clear();
    }

    /// Count of currently-live preview sources (leak-audit / test-support).
    #[cfg(test)]
    pub(crate) fn live_count_for_test(&self) -> usize {
        self.collection.live_count()
    }
}

fn member_key(group: &GroupId) -> MemberKey {
    MemberKey::new(format!("{}\u{0}{}", group.host_relay_url.as_str(), group.local_id))
}

fn open_group_preview(
    app: Arc<NmpApp>,
    projection: Arc<GroupTreeProjection>,
    group: GroupId,
) -> TeardownAction {
    let filter_json =
        serde_json::json!({ "kinds": [KIND_CHAT_MESSAGE], "#h": [&group.local_id] }).to_string();
    let Some(mut shape) = InterestShape::from_filter_json(&filter_json) else {
        return Box::new(|| {});
    };
    shape.relay_pin = Some(group.host_relay_url.clone());
    let observer_id = app.open_observed_projection(ObservedProjection::from_shape(
        Arc::clone(&projection) as Arc<dyn ObservedProjectionSink>,
        format!(
            "29er.group-tree-preview.{}.{}",
            group.host_relay_url.as_str(),
            group.local_id.as_str()
        ),
        GROUP_PREVIEW_SCOPE_GLOBAL,
        shape,
        GROUP_PREVIEW_REPLAY_LIMIT,
    ));
    if observer_id.0 == 0 {
        // D6 fail-closed: the kernel refused the declaration. Nothing was
        // opened, so nothing needs tearing down; the collection still treats
        // this key as live until the next reconcile diffs it away, matching
        // `KeyedReadCollection`'s "open never fails" contract (module docs).
        return Box::new(|| {});
    }
    let local_id = group.local_id.clone();
    Box::new(move || {
        app.close_observed_projection(observer_id);
        projection.remove_group(&local_id);
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use nmp_native_runtime::new_app;
    use nmp_nip29::projection::DiscoveredGroup;

    fn discovered(local_ids: &[&str]) -> DiscoveredGroupsSnapshot {
        DiscoveredGroupsSnapshot {
            host_relay_urls: vec!["wss://groups.example.com".to_string()],
            groups: local_ids
                .iter()
                .map(|local_id| DiscoveredGroup {
                    group_id: (*local_id).to_string(),
                    host_relay_url: "wss://groups.example.com".to_string(),
                    name: None,
                    picture: None,
                    about: None,
                    member_count: 0,
                    admin_count: 0,
                    public: true,
                    open: true,
                    parent: None,
                    children: Vec::new(),
                })
                .collect(),
        }
    }

    // Regression for the 29er#60 deadlock class: `sync` is a bare
    // `reconcile()` call, safe to run from any lane precisely because it
    // never runs from inside a snapshot-tick closure (see
    // `group_sessions.rs`). This test proves the mount/withdraw diff itself
    // is correct; the off-tick calling discipline is enforced by
    // `build_discovery_session` no longer capturing a `.sync()`-callable
    // handle in its registered closure at all.
    #[test]
    fn sync_mounts_one_source_per_discovered_group_and_withdraws_removed_ones() {
        let app = Arc::new(new_app());
        let projection = Arc::new(GroupTreeProjection::new());
        let sessions = GroupPreviewSessions::new(Arc::clone(&app), Arc::clone(&projection));

        sessions.sync(&discovered(&["room-a", "room-b"]));
        assert_eq!(sessions.collection.live_count(), 2);
        assert!(sessions.collection.full_recompute_matches());

        sessions.sync(&discovered(&["room-b"]));
        assert_eq!(
            sessions.collection.live_count(),
            1,
            "room-a dropped out of the discovered set must be withdrawn"
        );
        assert!(sessions.collection.full_recompute_matches());

        sessions.close_all(&projection);
        assert_eq!(sessions.collection.live_count(), 0);
    }

    #[test]
    fn sync_is_idempotent_for_an_unchanged_discovered_set() {
        let app = Arc::new(new_app());
        let projection = Arc::new(GroupTreeProjection::new());
        let sessions = GroupPreviewSessions::new(Arc::clone(&app), Arc::clone(&projection));

        let snapshot = discovered(&["room-a"]);
        sessions.sync(&snapshot);
        sessions.sync(&snapshot);
        sessions.sync(&snapshot);
        assert_eq!(sessions.collection.live_count(), 1);
    }
}
