//! 29er consumption of NMP-owned chat presence sessions.
//!
//! This module owns only app composition: which 29er groups should have
//! `nmp-chat` presence sessions open, and how their snapshots are folded into
//! the app-owned group-tree surface. Unread/read markers and typing semantics
//! remain in `nmp-chat`.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use nmp_chat::{
    close_chat_presence_session, open_chat_presence_session_with_reader, ChatPresenceHandle,
    ChatPresenceProjection, ChatPresenceSession, ChatRemoteTypingSpec, ReadMarker,
};
use nmp_native_runtime::NmpApp;
use nmp_nip29::projection::DiscoveredGroupsSnapshot;
use nmp_nip29::GroupId;

use crate::group_tree::{GroupTreeMessageSummary, GroupTreePresenceState};
use crate::kinds::{KIND_CHAT_MESSAGE, KIND_DISCUSSION_OR_ARTIFACT, KIND_TYPING_INDICATOR};

#[derive(Clone)]
struct PresenceEntry {
    group: GroupId,
    active_pubkey: String,
    handle: ChatPresenceHandle,
    reader: Arc<ChatPresenceProjection>,
}

/// Live NMP chat-presence sessions keyed by NIP-29 local group id.
#[derive(Default)]
pub struct GroupPresenceSessions {
    entries: Mutex<BTreeMap<String, PresenceEntry>>,
}

impl GroupPresenceSessions {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Reconcile live NMP presence sessions against the discovered group tree
    /// and active account. Empty account means no read-state can be computed
    /// correctly, so all presence sessions close fail-safe.
    pub fn sync(&self, app: &NmpApp, discovered: &DiscoveredGroupsSnapshot, active_pubkey: &str) {
        let Ok(mut entries) = self.entries.lock() else {
            return;
        };
        if active_pubkey.is_empty() {
            close_entries(app, &mut entries);
            return;
        }

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
                let wanted = desired.get(local_id)?;
                let still_current = entry.active_pubkey == active_pubkey
                    && entry.group.host_relay_url == wanted.host_relay_url
                    && entry.group.local_id == wanted.local_id;
                (!still_current).then(|| local_id.clone())
            })
            .chain(
                entries
                    .keys()
                    .filter(|local_id| !desired_keys.contains(*local_id))
                    .cloned(),
            )
            .collect();
        for local_id in stale {
            if let Some(entry) = entries.remove(&local_id) {
                let _ = close_chat_presence_session(app, entry.handle);
            }
        }

        for (local_id, group) in desired {
            if entries.contains_key(&local_id) {
                continue;
            }
            let descriptor = ChatPresenceSession::new(
                group.clone(),
                active_pubkey.to_string(),
                vec![KIND_CHAT_MESSAGE, KIND_DISCUSSION_OR_ARTIFACT],
            )
            .with_remote_typing(ChatRemoteTypingSpec::new(vec![KIND_TYPING_INDICATOR]));
            let (handle, reader) = open_chat_presence_session_with_reader(app, descriptor);
            entries.insert(
                local_id,
                PresenceEntry {
                    group,
                    active_pubkey: active_pubkey.to_string(),
                    handle,
                    reader,
                },
            );
        }
    }

    #[must_use]
    pub fn snapshot_state(&self) -> GroupTreePresenceState {
        let Ok(entries) = self.entries.lock() else {
            return GroupTreePresenceState::default();
        };
        let mut state = GroupTreePresenceState::default();
        for entry in entries.values() {
            state.apply_chat_presence_snapshot(entry.reader.snapshot());
        }
        state
    }

    /// Mark a group read to its latest known direct message boundary.
    ///
    /// Returns `false` when the session is absent, the group has no latest
    /// message yet, or the NMP projection rejects an older/equal marker.
    pub fn mark_read_to_latest(
        &self,
        local_id: &str,
        latest: Option<&GroupTreeMessageSummary>,
    ) -> bool {
        let Some(latest) = latest else {
            return false;
        };
        let Ok(entries) = self.entries.lock() else {
            return false;
        };
        entries
            .get(local_id)
            .map(|entry| {
                entry
                    .reader
                    .mark_read(ReadMarker::new(latest.id.clone(), latest.created_at))
            })
            .unwrap_or(false)
    }

    pub fn close_all(&self, app: &NmpApp) {
        if let Ok(mut entries) = self.entries.lock() {
            close_entries(app, &mut entries);
        }
    }
}

fn close_entries(app: &NmpApp, entries: &mut BTreeMap<String, PresenceEntry>) {
    for (_, entry) in std::mem::take(entries) {
        let _ = close_chat_presence_session(app, entry.handle);
    }
}
