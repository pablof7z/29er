//! Caller-side viewport gating for the group-tree keyed reads (29er#61).
//!
//! Per ADR-0078 ("viewport is an input, not intrinsic"): NMP's
//! `KeyedReadCollection` reconciles whatever desired-set it is handed — it
//! has no concept of visibility, and this module does NOT change that. 29er
//! is the caller that knows what's on screen, so 29er computes a
//! viewport-filtered desired-set and feeds THAT into
//! [`crate::group_preview::GroupPreviewSessions::sync`] /
//! [`crate::group_presence::GroupPresenceSessions::sync`] in place of the
//! full discovered-groups snapshot.
//!
//! [`GroupTreeViewport`] is the ONE shared implementation of that filter,
//! used by both shells that reconcile the group-tree collections —
//! `group_sessions::reconcile_group_tree_sessions` (iOS, via
//! [`crate::TwentyNinerApp`]) and the native Rust TUI's
//! `SharedProjections::sync_group_sources` (`crates/29er-tui/src/app.rs`) —
//! so the filtering policy lives in exactly one place, not two.

use std::collections::BTreeSet;
use std::sync::Mutex;

use nmp_nip29::projection::DiscoveredGroupsSnapshot;

/// Rows of look-ahead/behind kept open around each visible group, so a
/// one-row scroll doesn't immediately close/reopen a read at the
/// viewport's edge.
pub const VIEWPORT_BUFFER: usize = 3;

/// A shell-reported set of visible group-tree rows, by NIP-29 `local_id`
/// (`DiscoveredGroup::group_id`).
///
/// `None` — nothing reported yet — means "unknown", which [`Self::apply`]
/// treats as "show everything": the pre-#61 eager behavior, preserved by
/// default until a shell actually calls [`Self::set_visible`].
pub struct GroupTreeViewport {
    visible: Mutex<Option<BTreeSet<String>>>,
}

impl Default for GroupTreeViewport {
    fn default() -> Self {
        Self {
            visible: Mutex::new(None),
        }
    }
}

impl GroupTreeViewport {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Report the currently-visible local ids, replacing any previous
    /// report. An empty set means the shell has determined nothing is
    /// visible right now (e.g. an empty list) — [`Self::apply`] then keeps
    /// nothing open, which is correct, not a fail-safe gap.
    pub fn set_visible(&self, visible_local_ids: impl IntoIterator<Item = String>) {
        if let Ok(mut slot) = self.visible.lock() {
            *slot = Some(visible_local_ids.into_iter().collect());
        }
    }

    /// Revert to "unknown". [`Self::apply`] treats this the same as never
    /// having called [`Self::set_visible`] — the full discovered set.
    pub fn clear(&self) {
        if let Ok(mut slot) = self.visible.lock() {
            *slot = None;
        }
    }

    /// Filter `discovered` to the current viewport (module docs): every
    /// visible group plus [`VIEWPORT_BUFFER`] neighbors on each side. A
    /// cheap clone of `discovered` when no viewport has been reported yet.
    #[must_use]
    pub fn apply(&self, discovered: &DiscoveredGroupsSnapshot) -> DiscoveredGroupsSnapshot {
        let visible = self.visible.lock().ok().and_then(|slot| slot.clone());
        match visible {
            Some(visible) => filter_to_viewport(discovered, &visible, VIEWPORT_BUFFER),
            None => discovered.clone(),
        }
    }
}

/// Keep every group in `visible`, plus `buffer` neighbors on each side of it
/// in `discovered.groups`'s own stable order (`DiscoveredGroupsSnapshot`
/// is documented as sorted by `(host_relay_url, group_id)` — a deterministic
/// total order shared by every caller of `DiscoveredGroupsProjection::
/// snapshot`, so this needs no second tree-walk of its own to approximate
/// "nearby rows" and stay in sync with a shell's actual render order).
fn filter_to_viewport(
    discovered: &DiscoveredGroupsSnapshot,
    visible: &BTreeSet<String>,
    buffer: usize,
) -> DiscoveredGroupsSnapshot {
    let mut keep: BTreeSet<&str> = BTreeSet::new();
    for (idx, group) in discovered.groups.iter().enumerate() {
        if visible.contains(&group.group_id) {
            let lo = idx.saturating_sub(buffer);
            let hi = (idx + buffer + 1).min(discovered.groups.len());
            keep.extend(discovered.groups[lo..hi].iter().map(|g| g.group_id.as_str()));
        }
    }
    DiscoveredGroupsSnapshot {
        host_relay_urls: discovered.host_relay_urls.clone(),
        groups: discovered
            .groups
            .iter()
            .filter(|g| keep.contains(g.group_id.as_str()))
            .cloned()
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nmp_nip29::projection::DiscoveredGroup;

    fn discovered(local_ids: &[&str]) -> DiscoveredGroupsSnapshot {
        DiscoveredGroupsSnapshot {
            host_relay_urls: vec!["wss://groups.example.com".to_string()],
            groups: local_ids
                .iter()
                .map(|local_id| DiscoveredGroup {
                    group_id: (*local_id).to_string(),
                    host_relay_url: "wss://groups.example.com".to_string(),
                    ..Default::default()
                })
                .collect(),
        }
    }

    fn ids(snapshot: &DiscoveredGroupsSnapshot) -> Vec<&str> {
        snapshot.groups.iter().map(|g| g.group_id.as_str()).collect()
    }

    #[test]
    fn no_viewport_reported_passes_the_full_snapshot_through() {
        let viewport = GroupTreeViewport::new();
        let snapshot = discovered(&["a", "b", "c", "d", "e"]);
        assert_eq!(ids(&viewport.apply(&snapshot)), vec!["a", "b", "c", "d", "e"]);
    }

    #[test]
    fn viewport_keeps_visible_groups_plus_buffer_neighbors() {
        let viewport = GroupTreeViewport::new();
        // 10 groups, buffer=3: visible "e" (index 4) keeps indices 1..=7.
        let snapshot = discovered(&["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"]);
        viewport.set_visible(["e".to_string()]);
        assert_eq!(
            ids(&viewport.apply(&snapshot)),
            vec!["b", "c", "d", "e", "f", "g", "h"]
        );
    }

    #[test]
    fn viewport_excludes_groups_far_from_any_visible_row() {
        let viewport = GroupTreeViewport::new();
        let snapshot = discovered(&["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"]);
        viewport.set_visible(["a".to_string()]);
        let filtered = viewport.apply(&snapshot);
        let kept = ids(&filtered);
        assert!(!kept.contains(&"j"), "far-away group must be filtered out");
        assert!(kept.contains(&"a"));
    }

    #[test]
    fn empty_visible_set_keeps_nothing_open() {
        let viewport = GroupTreeViewport::new();
        let snapshot = discovered(&["a", "b", "c"]);
        viewport.set_visible(Vec::new());
        assert!(viewport.apply(&snapshot).groups.is_empty());
    }

    #[test]
    fn clear_reverts_to_the_full_snapshot() {
        let viewport = GroupTreeViewport::new();
        // 5 groups so a buffer=3 window around "a" (index 0) does NOT
        // already cover the whole set — otherwise `clear` would be a no-op
        // and this test wouldn't distinguish "filtered" from "cleared".
        let snapshot = discovered(&["a", "b", "c", "d", "e"]);
        viewport.set_visible(["a".to_string()]);
        assert_eq!(ids(&viewport.apply(&snapshot)), vec!["a", "b", "c", "d"]);
        viewport.clear();
        assert_eq!(ids(&viewport.apply(&snapshot)), vec!["a", "b", "c", "d", "e"]);
    }
}
