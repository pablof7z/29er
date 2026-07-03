//! 29er group-list preview source sessions.
//!
//! NMP owns routing and observed-projection delivery. This module only composes
//! one relay-pinned kind:9 source per discovered NIP-29 group into 29er's
//! app-owned group-tree preview projection.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use nmp_core::substrate::{ObservedProjection, ObservedProjectionRegistrar};
use nmp_core::{ObservedProjectionId, ObservedProjectionSink};
use nmp_native_runtime::NmpApp;
use nmp_nip29::projection::DiscoveredGroupsSnapshot;
use nmp_nip29::GroupId;
use nmp_planner::InterestShape;

use crate::group_tree::GroupTreeProjection;
use crate::kinds::KIND_CHAT_MESSAGE;

const GROUP_PREVIEW_REPLAY_LIMIT: usize = 80;
const GROUP_PREVIEW_SCOPE_GLOBAL: u32 = 1;

#[derive(Clone)]
struct PreviewEntry {
    group: GroupId,
    observer_id: ObservedProjectionId,
}

#[derive(Default)]
pub struct GroupPreviewSessions {
    entries: Mutex<BTreeMap<String, PreviewEntry>>,
}

impl GroupPreviewSessions {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn sync(
        &self,
        app: &NmpApp,
        discovered: &DiscoveredGroupsSnapshot,
        projection: Arc<GroupTreeProjection>,
    ) {
        let Ok(mut entries) = self.entries.lock() else {
            return;
        };

        let desired: BTreeMap<String, GroupId> = discovered
            .groups
            .iter()
            .filter(|group| !group.group_id.is_empty() && !group.host_relay_url.is_empty())
            .map(|group| {
                (
                    group.group_id.clone(),
                    GroupId::new(group.host_relay_url.clone(), group.group_id.clone()),
                )
            })
            .collect();
        let desired_keys: BTreeSet<String> = desired.keys().cloned().collect();

        let stale: Vec<String> = entries
            .iter()
            .filter_map(|(local_id, entry)| {
                let Some(wanted) = desired.get(local_id) else {
                    return Some(local_id.clone());
                };
                let still_current = entry.group.host_relay_url == wanted.host_relay_url
                    && entry.group.local_id == wanted.local_id;
                (!still_current).then(|| local_id.clone())
            })
            .collect();
        for local_id in stale {
            if let Some(entry) = entries.remove(&local_id) {
                app.close_observed_projection(entry.observer_id);
                projection.remove_group(&local_id);
            }
        }

        for local_id in entries
            .keys()
            .filter(|local_id| !desired_keys.contains(*local_id))
            .cloned()
            .collect::<Vec<_>>()
        {
            if let Some(entry) = entries.remove(&local_id) {
                app.close_observed_projection(entry.observer_id);
                projection.remove_group(&local_id);
            }
        }

        for (local_id, group) in desired {
            if entries.contains_key(&local_id) {
                continue;
            }
            let Some(observer_id) = open_group_preview(app, Arc::clone(&projection), &group) else {
                continue;
            };
            entries.insert(local_id, PreviewEntry { group, observer_id });
        }
    }

    pub fn close_all(&self, app: &NmpApp, projection: &GroupTreeProjection) {
        if let Ok(mut entries) = self.entries.lock() {
            for (_, entry) in std::mem::take(&mut *entries) {
                app.close_observed_projection(entry.observer_id);
            }
        }
        projection.clear();
    }
}

fn open_group_preview(
    app: &NmpApp,
    projection: Arc<GroupTreeProjection>,
    group: &GroupId,
) -> Option<ObservedProjectionId> {
    let filter_json =
        serde_json::json!({ "kinds": [KIND_CHAT_MESSAGE], "#h": [&group.local_id] }).to_string();
    let mut shape = InterestShape::from_filter_json(&filter_json)?;
    shape.relay_pin = Some(group.host_relay_url.clone());
    let observer_id = app.open_observed_projection(ObservedProjection::from_shape(
        projection as Arc<dyn ObservedProjectionSink>,
        format!(
            "29er.group-tree-preview.{}.{}",
            group.host_relay_url.as_str(),
            group.local_id.as_str()
        ),
        GROUP_PREVIEW_SCOPE_GLOBAL,
        shape,
        GROUP_PREVIEW_REPLAY_LIMIT,
    ));
    (observer_id.0 != 0).then_some(observer_id)
}
