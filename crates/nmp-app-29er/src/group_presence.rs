//! 29er consumption of NMP-owned chat presence sessions.
//!
//! This module owns only app composition: which 29er groups should have
//! `nmp-chat` presence sessions open, and how their snapshots are folded into
//! the app-owned group-tree surface. Unread/read markers and typing semantics
//! remain in `nmp-chat`.
//!
//! Membership is reconciled through an `nmp_read_session::KeyedReadCollection`
//! (#3115 shape (b), the read-session flavor) instead of a hand-diffed
//! `BTreeMap`. `active_pubkey` is NOT part of the key-set — it is carried
//! inside the per-key descriptor [`PresenceDescriptor`], so an identity
//! change diffs to a `Replace` of exactly the affected keys (Trellis
//! `PartialEq`-detects the divergence) instead of the previous force-close +
//! reopen of every row.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use nmp_chat::{
    close_chat_presence_session, open_chat_presence_session_with_reader, ChatPresenceProjection,
    ChatPresenceSession, ChatRemoteTypingSpec, ReadMarker,
};
use nmp_native_runtime::NmpApp;
use nmp_nip29::projection::DiscoveredGroupsSnapshot;
use nmp_nip29::GroupId;
use nmp_read_session::{KeyedReadCollection, MemberKey, TeardownAction};

use crate::group_tree::{GroupTreeMessageSummary, GroupTreePresenceState};
use crate::kinds::{KIND_CHAT_MESSAGE, KIND_DISCUSSION_OR_ARTIFACT, KIND_TYPING_INDICATOR};

/// One group's presence-session descriptor: the exogenous `active_pubkey`
/// scalar lives here (not in the key), so a value change on a live key
/// diffs to `Replace` (module docs, `KeyedReadCollection`).
#[derive(Clone, PartialEq, Eq)]
struct PresenceDescriptor {
    group: GroupId,
    active_pubkey: String,
}

/// Live NMP chat-presence sessions, reconciled through a
/// [`KeyedReadCollection`] keyed by `GroupId`. Each mounted key's reader is
/// additionally tracked in `readers` (pruned on withdrawal) — the
/// `KeyedReadCollection` primitive itself only owns open/close lifecycle, not
/// per-key output access, so callers who need the latter (as
/// [`Self::snapshot_state`] / [`Self::mark_read_to_latest`] do) track it
/// alongside via their own host `open` closure.
pub struct GroupPresenceSessions {
    collection: KeyedReadCollection<GroupId, PresenceDescriptor>,
    readers: Arc<Mutex<HashMap<String, Arc<ChatPresenceProjection>>>>,
}

impl GroupPresenceSessions {
    #[must_use]
    pub fn new(app: Arc<NmpApp>) -> Self {
        let readers: Arc<Mutex<HashMap<String, Arc<ChatPresenceProjection>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let readers_for_open = Arc::clone(&readers);
        let collection = KeyedReadCollection::new(
            "29er.group-tree.presence",
            member_key,
            move |key, descriptor: PresenceDescriptor| {
                open_group_presence(
                    Arc::clone(&app),
                    Arc::clone(&readers_for_open),
                    key,
                    descriptor,
                )
            },
        )
        .expect("fresh GroupPresenceSessions collection construction cannot fail");
        Self { collection, readers }
    }

    /// Reconcile live presence sessions against the discovered group tree and
    /// active account. Empty account means no read-state can be computed
    /// correctly, so all presence sessions close fail-safe.
    pub fn sync(&self, discovered: &DiscoveredGroupsSnapshot, active_pubkey: &str) {
        if active_pubkey.is_empty() {
            self.collection.close();
            return;
        }
        let desired: BTreeMap<GroupId, PresenceDescriptor> = discovered
            .groups
            .iter()
            .filter(|group| !group.group_id.is_empty() && !group.host_relay_url.is_empty())
            .map(|group| {
                let id = GroupId::new(group.host_relay_url.clone(), group.group_id.clone());
                (
                    id.clone(),
                    PresenceDescriptor {
                        group: id,
                        active_pubkey: active_pubkey.to_string(),
                    },
                )
            })
            .collect();
        self.collection.reconcile(desired);
    }

    #[must_use]
    pub fn snapshot_state(&self) -> GroupTreePresenceState {
        let Ok(readers) = self.readers.lock() else {
            return GroupTreePresenceState::default();
        };
        let mut state = GroupTreePresenceState::default();
        for reader in readers.values() {
            state.apply_chat_presence_snapshot(reader.snapshot());
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
        let Ok(readers) = self.readers.lock() else {
            return false;
        };
        readers
            .get(local_id)
            .map(|reader| reader.mark_read(ReadMarker::new(latest.id.clone(), latest.created_at)))
            .unwrap_or(false)
    }

    pub fn close_all(&self) {
        self.collection.close();
    }
}

fn member_key(group: &GroupId) -> MemberKey {
    MemberKey::new(format!("{}\u{0}{}", group.host_relay_url.as_str(), group.local_id))
}

fn open_group_presence(
    app: Arc<NmpApp>,
    readers: Arc<Mutex<HashMap<String, Arc<ChatPresenceProjection>>>>,
    _key: &MemberKey,
    descriptor: PresenceDescriptor,
) -> TeardownAction {
    // Readers are tracked by `local_id`, not the compound `MemberKey` — this
    // discovery session is scoped to one host relay (`GroupSessions::
    // open_discovery` takes a single `host_relay_url`), so `local_id` alone
    // is already the unique lookup [`Self::mark_read_to_latest`] wants, and
    // matches `GroupTreeProjection`/`GroupTreePresenceState`'s own
    // local-id-keyed maps.
    let local_id = descriptor.group.local_id.clone();
    let descriptor_session = ChatPresenceSession::new(
        descriptor.group,
        descriptor.active_pubkey,
        vec![KIND_CHAT_MESSAGE, KIND_DISCUSSION_OR_ARTIFACT],
    )
    .with_remote_typing(ChatRemoteTypingSpec::new(vec![KIND_TYPING_INDICATOR]));
    let (handle, reader) = open_chat_presence_session_with_reader(app.as_ref(), descriptor_session);
    if let Ok(mut readers_guard) = readers.lock() {
        readers_guard.insert(local_id.clone(), reader);
    }
    Box::new(move || {
        let _ = close_chat_presence_session(app.as_ref(), handle);
        if let Ok(mut readers_guard) = readers.lock() {
            readers_guard.remove(&local_id);
        }
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

    #[test]
    fn sync_mounts_one_session_per_discovered_group_and_withdraws_removed_ones() {
        let app = Arc::new(new_app());
        let sessions = GroupPresenceSessions::new(Arc::clone(&app));

        sessions.sync(&discovered(&["room-a", "room-b"]), "viewer-pubkey");
        assert_eq!(sessions.collection.live_count(), 2);
        assert!(sessions.collection.full_recompute_matches());
        assert_eq!(sessions.readers.lock().unwrap().len(), 2);

        sessions.sync(&discovered(&["room-b"]), "viewer-pubkey");
        assert_eq!(sessions.collection.live_count(), 1);
        assert!(sessions.collection.full_recompute_matches());
        // The withdrawn key's reader must be pruned, not leaked (module docs).
        assert_eq!(sessions.readers.lock().unwrap().len(), 1);

        sessions.close_all();
        assert_eq!(sessions.collection.live_count(), 0);
        assert!(sessions.readers.lock().unwrap().is_empty());
    }

    #[test]
    fn empty_active_pubkey_closes_all_sessions_fail_safe() {
        let app = Arc::new(new_app());
        let sessions = GroupPresenceSessions::new(Arc::clone(&app));
        sessions.sync(&discovered(&["room-a"]), "viewer-pubkey");
        assert_eq!(sessions.collection.live_count(), 1);

        sessions.sync(&discovered(&["room-a"]), "");
        assert_eq!(sessions.collection.live_count(), 0);
    }

    // An `active_pubkey` change on an unchanged key-set is the exogenous-
    // scalar `Replace` pattern (#3115 module docs) — the same `local_id` key
    // withdraws + remounts under the new pubkey rather than 29er's old
    // force-close+reopen of every row. `full_recompute_matches` proves no
    // owner-set divergence survives the swap.
    #[test]
    fn active_pubkey_change_replaces_the_affected_session_not_the_whole_collection() {
        let app = Arc::new(new_app());
        let sessions = GroupPresenceSessions::new(Arc::clone(&app));
        sessions.sync(&discovered(&["room-a"]), "pubkey-one");
        assert_eq!(sessions.collection.live_count(), 1);

        sessions.sync(&discovered(&["room-a"]), "pubkey-two");
        assert_eq!(sessions.collection.live_count(), 1);
        assert!(sessions.collection.full_recompute_matches());
        assert_eq!(sessions.readers.lock().unwrap().len(), 1);
    }
}
