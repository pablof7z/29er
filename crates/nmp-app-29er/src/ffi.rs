//! The `pub extern "C"` registration + NIP-29 group-discovery entry points
//! the 29er iOS shell links against. Mirrors `nmp-app-chirp::ffi` but strips
//! to the S01 surface: register, open/close group discovery, register group
//! chat, dispatch action bytes, declare consumed projections, unregister.

use std::ffi::c_char;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use nmp_core::dispatch_envelope::{encode_dispatch_envelope, DISPATCH_ENVELOPE_SCHEMA_VERSION};
use nmp_core::substrate::{ActionPayload, ObservedProjection, ObservedProjectionRegistrar};
use nmp_core::{ObservedProjectionId, ObservedProjectionSink, TypedProjectionData};
use nmp_ffi::{nmp_app_add_relay, nmp_app_dispatch_action_bytes, nmp_app_remove_relay, NmpApp};
use nmp_nip29::group_id::GroupId;
use nmp_nip29::kinds::{KIND_CHAT_MESSAGE, KIND_DISCUSSION_OR_ARTIFACT};

use crate::group_tree::{
    encode_group_tree_snapshot, membership_from_joined, GroupMembershipMap, GroupTreeProjection,
    GROUP_TREE_FILE_IDENTIFIER, GROUP_TREE_SCHEMA_ID, GROUP_TREE_SCHEMA_VERSION,
};
use crate::relay_selector::{register_relay_selector_runtime, RelaySelectorProjection};

/// Opaque handle returned by [`nmp_app_29er_register`] and consumed by
/// [`nmp_app_29er_unregister`]. Boxed on the heap; the pointer is opaque to C.
pub struct TwentyNinerHandle {
    #[allow(dead_code)]
    app: *mut NmpApp,
    relay_selector: Arc<RelaySelectorProjection>,
}

pub struct TwentyNinerGroupDiscoveryHandle {
    /// Kernel observer id of the 29er-owned kind:9 group-tree observed
    /// projection (the only tap 29er opens directly; the discovered/joined
    /// catalogs are owned by the NMP doors). Recorded for symmetry/introspection;
    /// the `teardown_fn` closure captures its own copy to close it, so the field
    /// itself is not read back.
    #[allow(dead_code)]
    tree_observer_id: ObservedProjectionId,
    /// The 29er kind:9 reader Arc — kept reachable so
    /// [`nmp_app_29er_mark_group_read`] can fold read state into the next tree
    /// snapshot.
    tree_projection: Arc<GroupTreeProjection>,
    teardown_fn: Box<dyn FnOnce() + Send>,
}

unsafe impl Send for TwentyNinerGroupDiscoveryHandle {}
unsafe impl Sync for TwentyNinerGroupDiscoveryHandle {}

// SAFETY: `TwentyNinerHandle` only carries a `*mut NmpApp` whose lifetime is
// managed by the caller (the Swift shell frees the app via `nmp_app_free`
// after `nmp_app_29er_unregister`). The handle is never sent across threads
// in the 29er shell; `Send` is declared so the `extern "C"` surface stays
// uniform with Chirp's handle.
unsafe impl Send for TwentyNinerHandle {}

/// Status code returned by [`nmp_app_29er_register`].
///
/// `#[repr(u32)]` so it maps to a plain `uint32_t` in C / Swift. Discriminants
/// are stable — do not renumber; add new variants at the end only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum NmpRegisterStatus {
    /// Registration succeeded. `handle_out` is non-null and ready to use.
    Ok = 0,
    /// The `app` pointer was null. `handle_out` is left as null.
    NullApp = 1,
}

/// Register 29er's NIP-29 projections + the canonical NMP default composition
/// against `app`.
///
/// Returns a [`NmpRegisterStatus`] discriminant as `u32`. On
/// [`NmpRegisterStatus::Ok`] the opaque handle is written through `handle_out`;
/// on any failure `*handle_out` is left unchanged (the caller should
/// initialise it to `NULL` before calling).
///
/// # Safety
///
/// * `app` must be a valid non-null `*mut NmpApp` from `nmp_app_new()`.
/// * `handle_out` must be a valid non-null `*mut *mut TwentyNinerHandle`;
///   passing null returns [`NmpRegisterStatus::NullApp`] without writing.
/// * `app` MUST outlive the returned handle. Call
///   [`nmp_app_29er_unregister`] before `nmp_app_free`.
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_29er_register(
    app: *mut NmpApp,
    handle_out: *mut *mut TwentyNinerHandle,
) -> u32 {
    if app.is_null() {
        return NmpRegisterStatus::NullApp as u32;
    }

    // Inherit canonical NMP composition once. The default set wires the NIP-02
    // / NIP-17 / NIP-57 / NIP-65 action modules, the production routing
    // substrate, the DM-inbox + zap-receipts runtimes, and the D2 coverage
    // hook. 29er is a full client (not a notes-only app), so the full default
    // set is the right baseline.
    //
    // SAFETY: caller guarantees `app` is a valid pointer allocated by
    // `nmp_app_new` for the duration of this call. We hold an exclusive
    // `&mut *app` only across this block; no other reference aliases it.
    let _default_handles = nmp_defaults::register_defaults_with_handles(
        unsafe { &mut *app },
        nmp_defaults::NmpDefaults::default(),
    );

    // 29er-specific: register the NIP-29 action namespaces against the kernel
    // action registry. Lives in this crate (not the template) because NIP-29
    // is not part of the canonical NMP composition every Nostr app inherits —
    // a notes-only app would not register it. Same rationale as Chirp's
    // `register_nip29_actions`.
    //
    // SAFETY: same exclusive-borrow rationale as `register_defaults` — no
    // other reference aliases `app` at this point.
    // Group-publishing actions now read recent group events for `["previous", …]`
    // tags through the execution `ActionContext` at dispatch time (#2140), so
    // `register_actions` no longer takes the event-store publish-back handle.
    let _ = nmp_nip29::register::register_actions(unsafe { &mut *app });

    // Wire the NIP-29 group-create defaults projection so 29er's suggested
    // public-group relay URL surfaces under `"nmp.nip29.group_defaults"`. The
    // relay is 29er operator policy ([`crate::config::public_group_relay_url`]),
    // threaded in via `wire_group_defaults_with_relay` so the shell reads it
    // from the projection instead of hardcoding it (D7). Output-only: the
    // projection observes no kernel events — its snapshot is a pure function of
    // the supplied URL — so this is a one-time registration at app init (same
    // pattern as Chirp's `wire_group_defaults_with_relay`).
    //
    // SAFETY: shared-ref borrow; the projection registration is internally
    // lock-guarded.
    nmp_nip29::register::wire_group_defaults_with_relay(
        unsafe { &*app },
        crate::config::public_group_relay_url(),
    );
    let relay_selector = register_relay_selector_runtime(unsafe { &*app });

    // D6 — guard the write-through before allocating the handle. A null
    // `handle_out` is a programmer-error contract violation; returning
    // `NullApp` (the same discriminant Chirp uses for this case) is the safe,
    // D6-compliant behaviour.
    if handle_out.is_null() {
        return NmpRegisterStatus::NullApp as u32;
    }
    let handle = Box::into_raw(Box::new(TwentyNinerHandle {
        app,
        relay_selector,
    }));
    // SAFETY: `handle_out` was verified non-null above; the pointer must be a
    // valid `*mut *mut TwentyNinerHandle` per the function's SAFETY contract.
    unsafe { *handle_out = handle };
    NmpRegisterStatus::Ok as u32
}

/// Tear down the 29er registration handle returned by
/// [`nmp_app_29er_register`].
///
/// The handle is reclaimed; the pointer MUST NOT be used after this call.
/// D6 — a null `handle` is a silent no-op.
///
/// # Safety
///
/// `handle` must be a valid pointer returned by `nmp_app_29er_register` or
/// null. Call this before `nmp_app_free`.
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_29er_unregister(handle: *mut TwentyNinerHandle) {
    if handle.is_null() {
        return;
    }
    // SAFETY: `handle` is a valid pointer returned by
    // `nmp_app_29er_register` and must not be used after this call.
    // `Box::from_raw` takes ownership; the drop reclaims the allocation.
    // The `app` pointer inside is NOT freed here — the caller still owns it
    // and frees it via `nmp_app_free` after this returns.
    unsafe { drop(Box::from_raw(handle)) };
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_29er_relay_selector_select_relay(
    handle: *mut TwentyNinerHandle,
    relay_url: *const c_char,
) -> bool {
    let Some((app, relay_selector, relay_url)) = relay_selector_args(handle, relay_url) else {
        return false;
    };
    let Some(selected) = relay_selector.select_relay(&relay_url) else {
        return false;
    };
    seed_relay(app, &selected);
    true
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_29er_relay_selector_add_relay(
    handle: *mut TwentyNinerHandle,
    relay_url: *const c_char,
) -> bool {
    let Some((app, relay_selector, relay_url)) = relay_selector_args(handle, relay_url) else {
        return false;
    };
    let tx = unsafe { &*app }.actor_sender();
    let Some(added) = relay_selector.add_relay(&relay_url, &tx) else {
        return false;
    };
    seed_relay(app, &added);
    true
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_29er_relay_selector_remove_relay(
    handle: *mut TwentyNinerHandle,
    relay_url: *const c_char,
) -> bool {
    let Some((app, relay_selector, relay_url)) = relay_selector_args(handle, relay_url) else {
        return false;
    };
    let tx = unsafe { &*app }.actor_sender();
    let Some(removed) = relay_selector.remove_relay(&relay_url, &tx) else {
        return false;
    };
    if removed != crate::config::public_group_relay_url() {
        let Ok(url) = std::ffi::CString::new(removed) else {
            return true;
        };
        nmp_app_remove_relay(app, url.as_ptr());
    }
    true
}

fn relay_selector_args(
    handle: *mut TwentyNinerHandle,
    relay_url: *const c_char,
) -> Option<(*mut NmpApp, Arc<RelaySelectorProjection>, String)> {
    if handle.is_null() {
        return None;
    }
    let relay_url = c_string_opt(relay_url).filter(|value| !value.trim().is_empty())?;
    let handle = unsafe { &*handle };
    Some((handle.app, Arc::clone(&handle.relay_selector), relay_url))
}

fn seed_relay(app: *mut NmpApp, relay_url: &str) {
    let (Ok(url), Ok(role)) = (
        std::ffi::CString::new(relay_url),
        std::ffi::CString::new("both"),
    ) else {
        return;
    };
    nmp_app_add_relay(app, url.as_ptr(), role.as_ptr());
}

/// Declare that this host consumes all kernel-owned built-in Tier-2
/// projections. 29er is a full client, so it follows Chirp's posture: call
/// [`nmp_ffi::nmp_app_consume_all_builtin_projections`] once at app init,
/// before [`nmp_ffi::nmp_app_start`]. A null `app` is a silent no-op (D6).
///
/// # Safety
///
/// `app` must be a valid non-null `*mut NmpApp` from `nmp_app_new()` or null.
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_29er_declare_consumed_projections(app: *mut NmpApp) {
    if app.is_null() {
        return;
    }
    // SAFETY: caller guarantees `app` is a valid pointer from `nmp_app_new`,
    // live for the duration of this call. The borrow is not held past return.
    nmp_ffi::nmp_app_consume_all_builtin_projections(unsafe { &mut *app });
}

/// Wire a NIP-29 `GroupEventsProjection` for a single chat group into `app`.
///
/// Pure consumption — the read side of a group-chat screen. Adds no new C-ABI
/// handle and registers no actions. `group_id_json` is a JSON object naming
/// the target group:
///
/// ```json
/// {"host_relay_url":"wss://groups.example.com","local_id":"room"}
/// ```
///
/// D6 — fire-and-forget. A null `app`, a null/invalid-UTF-8 `group_id_json`,
/// a JSON shape that does not deserialize to a [`GroupId`], or a poisoned
/// observer slot all degrade to a silent return.
///
/// # Safety
///
/// `app` must be a valid non-null `*mut NmpApp` from `nmp_app_new()` or null.
/// `group_id_json` may be null. `app` MUST outlive the registration; it is
/// borrowed only for the duration of this call.
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_29er_register_group_chat(app: *mut NmpApp, group_id_json: *const c_char) {
    if app.is_null() {
        return;
    }
    // SAFETY: caller guarantees `app` is a valid pointer from `nmp_app_new`,
    // live for the duration of this call. The borrow is not held past return.
    let app_ref = unsafe { &*app };

    let Some(raw) = c_string_opt(group_id_json) else {
        return;
    };
    let Ok(group_id) = serde_json::from_str::<GroupId>(&raw) else {
        return;
    };

    // The per-open chat read view is `open_group_events`, where the CONSUMER
    // declares which kinds it wants — NIP-29 owns only the `["h", local_id]`
    // routing concern. 29er's chat screen reads chat (kind 9) + thread/
    // discussion (kind 11), so it passes those two explicitly. The door
    // registers the NGEV typed sidecar + the (muted) `GroupEventsProjection`,
    // replays the read cache, and opens the relay-pinned tailing interest.
    app_ref.open_group_events(group_id, vec![KIND_CHAT_MESSAGE, KIND_DISCUSSION_OR_ARTIFACT]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn m002_membership_and_admin_namespaces_encode_typed_payloads() {
        let leave = encode_payload_for_namespace(
            "nmp.nip29.leave",
            r#"{"group":{"host_relay_url":"wss://groups.example.com","local_id":"room"},"reason":"done"}"#,
        )
        .expect("leave payload encodes");
        let leave =
            <nmp_nip29::action::LeaveGroupInput as ActionPayload>::decode(&leave).expect("decodes");
        assert_eq!(leave.group.local_id, "room");
        assert_eq!(leave.reason.as_deref(), Some("done"));

        let create = encode_payload_for_namespace(
            "nmp.nip29.create_public_group",
            r#"{"group":{"host_relay_url":"wss://groups.example.com","local_id":"child-room"},"name":"Child Room","about":"Admin work","visibility":"private","access":"closed","parent":"root"}"#,
        )
        .expect("create payload encodes");
        let create = <nmp_nip29::action::CreatePublicGroupInput as ActionPayload>::decode(&create)
            .expect("decodes");
        assert_eq!(create.group.local_id, "child-room");
        assert_eq!(create.name, "Child Room");
        assert_eq!(create.parent.as_deref(), Some("root"));
        assert_eq!(create.about.as_deref(), Some("Admin work"));

        let put_user = encode_payload_for_namespace(
            "nmp.nip29.put_user",
            r#"{"group":{"host_relay_url":"wss://groups.example.com","local_id":"room"},"target_pubkey":"0000000000000000000000000000000000000000000000000000000000000001","role":"admin","reason":"trusted"}"#,
        )
        .expect("put-user payload encodes");
        let put_user =
            <nmp_nip29::action::PutUserInput as ActionPayload>::decode(&put_user).expect("decodes");
        assert_eq!(
            put_user.target_pubkey,
            "0000000000000000000000000000000000000000000000000000000000000001"
        );
        assert_eq!(put_user.role.as_deref(), Some("admin"));

        let invite = encode_payload_for_namespace(
            "nmp.nip29.create_invite",
            r#"{"group":{"host_relay_url":"wss://groups.example.com","local_id":"room"},"codes":["alpha","beta"]}"#,
        )
        .expect("invite payload encodes");
        let invite = <nmp_nip29::action::CreateInviteInput as ActionPayload>::decode(&invite)
            .expect("decodes");
        assert_eq!(invite.codes, vec!["alpha".to_string(), "beta".to_string()]);

        let set_parent = encode_payload_for_namespace(
            "nmp.nip29.set_parent",
            r#"{"group":{"host_relay_url":"wss://groups.example.com","local_id":"child-room"},"parent":"root"}"#,
        )
        .expect("set-parent payload encodes");
        let set_parent = <nmp_nip29::action::SetParentInput as ActionPayload>::decode(&set_parent)
            .expect("decodes");
        assert_eq!(set_parent.group.local_id, "child-room");
        assert_eq!(set_parent.parent.as_deref(), Some("root"));
    }

    #[test]
    fn chat_send_doorway_composes_kind9_publish_group_event() {
        const HEX: &str = "3bf0c63fcb93463407af97a5e5ee64fa883d107ef9e558472c4eb9aaaefa459d";
        let npub = nmp_core::nip19::encode_npub(HEX).expect("valid hex");
        let bytes = encode_chat_send_payload(&format!(
            r#"{{"group":{{"host_relay_url":"wss://groups.example.com","local_id":"room"}},"content":"hi @{HEX}","mention_pubkeys":["{HEX}"]}}"#
        ))
        .expect("chat-send composes");
        let input = <nmp_nip29::action::PublishGroupEventInput as ActionPayload>::decode(&bytes)
            .expect("decodes");
        assert_eq!(input.group.local_id, "room");
        assert_eq!(input.kind, KIND_CHAT_MESSAGE);
        assert_eq!(input.content, format!("hi nostr:{npub}"));
        assert_eq!(input.tags, vec![vec!["p".to_string(), HEX.to_string()]]);
        // The envelope tags (`h` / `previous`) must NOT be present — nmp-nip29
        // injects them and rejects caller-supplied copies.
        assert!(input
            .tags
            .iter()
            .all(|t| t.first().map(String::as_str) != Some("h")));
        assert!(input
            .tags
            .iter()
            .all(|t| t.first().map(String::as_str) != Some("previous")));
    }

    #[test]
    fn chat_send_doorway_fails_closed_on_malformed_body() {
        assert!(encode_chat_send_payload(r#"{"content":"no group"}"#).is_none());
    }

    #[test]
    fn action_encoder_fails_closed_for_unknown_or_malformed_payloads() {
        assert!(encode_payload_for_namespace("nmp.nip29.remove_everyone", "{}").is_none());
        assert!(encode_payload_for_namespace("nmp.nip29.leave", r#"{"group":42}"#).is_none());
    }
}

/// Open a NIP-29 group-discovery session for one host relay.
///
/// The **read side** of the NIP-29 group-discovery flow. Opens NMP's canonical
/// hydrating doors — `open_group_discovery_with_reader` (owns
/// `DiscoveredGroupsProjection` + the `"nmp.nip29.discovered_groups"` sidecar
/// + relay-pinned interest + #2088 replay) and
/// `open_joined_groups_with_reader` (account-scoped `is_member`/`is_admin`
/// truth) — and layers ONE 29er-owned read on top: a hydrating, relay-pinned
/// kind:9 observed projection feeding the `GroupTreeProjection` (per-group
/// unread + last-message preview). It composes those canonical snapshots into
/// the app-owned `"nmp.29er.group_tree"` typed projection on every tick.
///
/// Returns a heap-allocated opaque handle the caller MUST free via
/// [`nmp_app_29er_close_group_discovery`]. A null `app`, null/non-UTF-8/empty
/// `host_relay_url`, or a kernel that refuses the kind:9 observed projection
/// returns NULL (D6).
///
/// # Safety
///
/// `app` must be a valid non-null `*mut NmpApp` from `nmp_app_new()` or null.
/// `host_relay_url` may be null. `app` MUST outlive the returned handle; call
/// [`nmp_app_29er_close_group_discovery`] before `nmp_app_free`.
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_29er_open_group_discovery(
    app: *mut NmpApp,
    host_relay_url: *const c_char,
) -> *mut TwentyNinerGroupDiscoveryHandle {
    if app.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: caller guarantees `app` is a valid pointer from `nmp_app_new`,
    // live for the duration of this call and the returned handle.
    let app_ref = unsafe { &*app };

    let Some(relay_url) = c_string_opt(host_relay_url).filter(|s| !s.is_empty()) else {
        return std::ptr::null_mut();
    };

    match open_group_discovery_with_tree(app_ref, relay_url) {
        Some(handle) => Box::into_raw(Box::new(handle)),
        None => std::ptr::null_mut(),
    }
}

/// Bounded read-cache replay budget for the 29er kind:9 group-tree observed
/// projection. Mirrors `nmp_feed::DEFAULT_FEED_WINDOW_LIMIT` (80) — kept as a
/// local const so this crate does not take an `nmp-feed` dependency just for
/// one number. The cap bounds how many cached kind:9 events are replayed into
/// the muted observer before it activates live (the #2088 hydration sequence).
const GROUP_TREE_REPLAY_LIMIT: usize = 80;

fn open_group_discovery_with_tree(
    app: &NmpApp,
    relay_url: String,
) -> Option<TwentyNinerGroupDiscoveryHandle> {
    // 1. Canonical discovery door — NMP owns `DiscoveredGroupsProjection`, the
    //    `nmp.nip29.discovered_groups` sidecar, the relay-pinned interest, and
    //    the #2088 hydrating replay. We retain the returned snapshot reader
    //    (Arc) to compose the 29er tree.
    let (discovery_handle, discovered) = app.open_group_discovery_with_reader(relay_url.clone());

    // 2. Viewer membership/admin truth comes from the account-scoped joined-
    //    groups door (D11 — the app crate owns membership/admin derivation, not
    //    a Swift roster scan). The door bakes the active pubkey + relay pin and
    //    is `ActiveAccount`-scoped(0). An empty active pubkey skips the open
    //    (the door itself also no-ops on empty), yielding `None` — membership is
    //    simply not surfaced until an account is active. The active_account slot
    //    is re-read at snapshot time so a later account switch fails safe (see
    //    `membership_from_joined`).
    let active_account = app.active_account_handle();
    let active_pubkey = active_account
        .lock()
        .ok()
        .and_then(|slot| slot.clone())
        .unwrap_or_default();
    let joined = if active_pubkey.is_empty() {
        None
    } else {
        app.open_joined_groups_with_reader(active_pubkey, relay_url.clone())
    };

    // 3. 29er-owned kind:9 unread/preview reader — folding kind:9 into per-group
    //    unread + last-message preview is legitimate per-app composition. It is
    //    sourced through a hydrating, relay-pinned observed projection with
    //    explicit lifecycle ownership. This gives the same muted -> replay-cached
    //    -> activate-live sequence the NMP doors use, so a tree opened after kind:9
    //    was already cached hydrates correctly (#2088), and it is torn down via
    //    `close_observed_projection`. Tighter `#h` filtering is impossible at
    //    discovery-open time (group ids are unknown until the catalog arrives),
    //    so kinds:[9] pinned to the host relay is the correct shape.
    let tree_messages = Arc::new(GroupTreeProjection::new());
    let filter_json = format!(r#"{{"kinds":[{KIND_CHAT_MESSAGE}]}}"#);
    let replay_shapes: Vec<nmp_planner::InterestShape> =
        nmp_planner::InterestShape::from_filter_json(&filter_json)
            .map(|mut shape| {
                shape.relay_pin = Some(relay_url.clone());
                shape
            })
            .into_iter()
            .collect();
    let tree_observer_id = app.open_observed_projection(ObservedProjection {
        observer: Arc::clone(&tree_messages) as Arc<dyn ObservedProjectionSink>,
        filter_json,
        consumer_id: format!("29er.nip29.group_tree.kind9:{relay_url}"),
        // Global, relay-pinned — matches the discovery door's SCOPE_GLOBAL.
        scope: 1,
        relay_pin: Some(relay_url.clone()),
        replay_shapes,
        replay_limit: GROUP_TREE_REPLAY_LIMIT,
    });
    if tree_observer_id.0 == 0 {
        // Roll back the doors we already opened (D6 fail-closed).
        app.close_group_feed_token(discovery_handle);
        app.close_joined_groups();
        return None;
    }

    // 4. 29er composite — derive `nmp.29er.group_tree` from the canonical
    //    discovered/joined door snapshots + the app-owned kind:9 summaries. This
    //    is app composition over canonical NMP outputs (allowed); it registers
    //    no kernel taps and re-uses no NMP-owned keys.
    let tree_discovered = Arc::clone(&discovered);
    let tree_joined = joined.clone();
    let tree_messages_for_sidecar = Arc::clone(&tree_messages);
    let active_account_for_tree = Arc::clone(&active_account);
    app.register_typed_snapshot_projection("nmp.29er.group_tree", move || {
        let snapshot = tree_discovered.snapshot();
        let messages = tree_messages_for_sidecar.snapshot();
        // Re-read the live active pubkey every tick so membership is recomputed
        // on an account switch and never leaks a previous account's truth.
        let active_pubkey = active_account_for_tree
            .lock()
            .ok()
            .and_then(|slot| slot.clone())
            .unwrap_or_default();
        let joined_snapshot = tree_joined
            .as_ref()
            .map(|projection| projection.snapshot())
            .unwrap_or_default();
        let membership: GroupMembershipMap =
            membership_from_joined(&joined_snapshot, &active_pubkey);
        Some(TypedProjectionData {
            key: "nmp.29er.group_tree".to_string(),
            schema_id: GROUP_TREE_SCHEMA_ID.to_string(),
            schema_version: GROUP_TREE_SCHEMA_VERSION,
            file_identifier: String::from_utf8_lossy(GROUP_TREE_FILE_IDENTIFIER).into_owned(),
            payload: encode_group_tree_snapshot(&snapshot, &messages, &membership),
            ..Default::default()
        })
    });

    let app_addr = (app as *const NmpApp) as usize;
    Some(TwentyNinerGroupDiscoveryHandle {
        tree_observer_id,
        tree_projection: tree_messages,
        teardown_fn: Box::new(move || {
            // SAFETY: `app_addr` is the address of the live `NmpApp` the open
            // borrowed; the handle's contract requires `app` to outlive it.
            let app = unsafe { &*(app_addr as *const NmpApp) };
            app.close_observed_projection(tree_observer_id);
            app.remove_snapshot_projection("nmp.29er.group_tree");
            // The NMP doors own `nmp.nip29.discovered_groups` / `…joined_groups`
            // + their interests; close reclaims them. 29er must NOT remove those
            // keys itself (it would race/clobber the door session).
            app.close_group_feed_token(discovery_handle);
            app.close_joined_groups();
        }),
    })
}

/// Close a NIP-29 group-discovery session opened by
/// [`nmp_app_29er_open_group_discovery`].
///
/// Closes the 29er kind:9 observed projection + the `"nmp.29er.group_tree"`
/// typed snapshot, then closes NMP's discovery + joined-groups doors (which own
/// and reclaim `"nmp.nip29.discovered_groups"` / `"nmp.nip29.joined_groups"` +
/// their interests) so no stale group catalog is emitted after the discover
/// screen is dismissed. The handle memory is reclaimed; the pointer MUST NOT be
/// used after this call.
///
/// D6 — a null `handle` is a silent no-op.
///
/// # Safety
///
/// `handle` must be a valid pointer returned by
/// `nmp_app_29er_open_group_discovery` or null.
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_29er_close_group_discovery(handle: *mut TwentyNinerGroupDiscoveryHandle) {
    if handle.is_null() {
        return;
    }
    // SAFETY: `handle` is a valid pointer returned by
    // `nmp_app_29er_open_group_discovery` and must not be used after this
    // call. `Box::from_raw` takes ownership; `close_group_discovery` tears
    // down the observer + projection before the box is dropped.
    let handle = unsafe { *Box::from_raw(handle) };
    (handle.teardown_fn)();
}

/// Mark a group's direct kind:9 messages read inside the open group-tree
/// discovery projection. The next tree snapshot will fold that read state into
/// the group's aggregate unread count.
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_29er_mark_group_read(
    handle: *mut TwentyNinerGroupDiscoveryHandle,
    group_id: *const c_char,
) {
    if handle.is_null() {
        return;
    }
    let Some(group_id) = c_string_opt(group_id).filter(|s| !s.is_empty()) else {
        return;
    };
    let handle = unsafe { &*handle };
    handle.tree_projection.mark_read(&group_id);
}

/// Select a group's member roster.
///
/// v0.8.0: the per-member `GroupMembersProjection` was removed from `nmp-nip29`
/// (membership now derives from `JoinedGroupsProjection` and the per-relay
/// `DiscoveredGroup.member_count`/`admin_count`). The dedicated roster door no
/// longer exists, so this entry point is retained for C-ABI stability as a
/// validated no-op until 29er builds its own roster observer.
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_29er_select_group_members(
    handle: *mut TwentyNinerGroupDiscoveryHandle,
    group_id: *const c_char,
) {
    if handle.is_null() {
        return;
    }
    let Some(_group_id) = c_string_opt(group_id).filter(|s| !s.is_empty()) else {
        return;
    };
}

/// Process-local correlation-id source for 29er's byte-doorway dispatches.
///
/// Mirrors `nmp-app-chirp::dispatch_bytes::mint_correlation_id`: a monotone
/// atomic counter satisfies the "unique within one running process for the
/// lifetime of an in-flight operation" contract with zero extra deps. The
/// `29er-` prefix namespaces it so it never collides with the kernel's hex
/// correlation ids.
static NEXT_CORRELATION_ID: AtomicU64 = AtomicU64::new(1);

pub(crate) fn mint_correlation_id() -> String {
    let n = NEXT_CORRELATION_ID.fetch_add(1, Ordering::Relaxed);
    format!("29er-{n}")
}

/// Dispatch a NIP-29 action through the typed byte doorway.
///
/// Builds the typed payload for `namespace` from `body_json` (the canonical
/// action body), mints a host correlation id, wraps payload + namespace + id
/// in an open [`nmp_core::dispatch_envelope::DispatchEnvelope`], and hands the
/// finished bytes to [`nmp_ffi::nmp_app_dispatch_action_bytes`]. Returns a
/// freshly heap-allocated, NUL-terminated JSON C string the caller MUST
/// release via [`nmp_ffi::nmp_free_string`]: `{"correlation_id":"<id>"}` on
/// accept, or `{"error":"<message>"}` on rejection.
///
/// Fail-closed (D6): a null `app`, a null `body_json`, an unknown namespace, or
/// a body that does not deserialize into the namespace's typed action all
/// return `{"error":…}` — never NULL for a non-null `app`, never a panic.
///
/// # Safety
///
/// `app` must be a valid non-null `*mut NmpApp` from `nmp_app_new()` or null.
/// `namespace` and `body_json` may be null. The returned pointer is heap-
/// allocated by Rust and MUST be freed via `nmp_free_string`.
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_29er_dispatch_action_bytes(
    app: *mut NmpApp,
    namespace: *const c_char,
    body_json: *const c_char,
) -> *mut c_char {
    use std::ffi::CString;

    if app.is_null() {
        return CString::new(r#"{"error":"null app"}"#)
            .unwrap_or_else(|_| c"{}".to_owned())
            .into_raw();
    }
    let Some(ns) = c_string_opt(namespace) else {
        return CString::new(r#"{"error":"null namespace"}"#)
            .unwrap_or_else(|_| c"{}".to_owned())
            .into_raw();
    };
    let Some(body) = c_string_opt(body_json) else {
        return CString::new(r#"{"error":"null body"}"#)
            .unwrap_or_else(|_| c"{}".to_owned())
            .into_raw();
    };

    // The 29er chat-send doorway is the one place that composes. The shell
    // hands raw text + `@mention` pubkeys under `CHAT_SEND_NAMESPACE`; we run
    // the shared `compose_chat_message` (NIP-21 rewrite + `p` tags), wrap the
    // result as a kind:9 `PublishGroupEventInput`, and emit it under the real
    // `nmp.nip29.publish_group_event` action namespace. NIP-29 injects the
    // `h`/`previous` envelope itself — we must not (it rejects caller-supplied
    // envelope tags). Every other namespace re-encodes its body verbatim.
    let (dispatch_ns, payload): (&str, Vec<u8>) = if ns == CHAT_SEND_NAMESPACE {
        match encode_chat_send_payload(&body) {
            Some(p) => (PUBLISH_GROUP_EVENT_NAMESPACE, p),
            None => {
                return CString::new(r#"{"error":"could not compose chat message from body"}"#)
                    .unwrap_or_else(|_| c"{}".to_owned())
                    .into_raw();
            }
        }
    } else {
        match encode_payload_for_namespace(&ns, &body) {
            Some(p) => (ns.as_str(), p),
            None => {
                return CString::new(format!(
                    r#"{{"error":"no typed payload encoder for action namespace '{ns}'"}}"#
                ))
                .unwrap_or_else(|_| c"{}".to_owned())
                .into_raw();
            }
        }
    };

    let correlation_id = mint_correlation_id();
    let envelope = encode_dispatch_envelope(
        &correlation_id,
        dispatch_ns,
        DISPATCH_ENVELOPE_SCHEMA_VERSION,
        &payload,
    );

    // SAFETY: `app` is a valid, non-null pointer (checked above); `envelope`
    // is a live, fully-initialised byte buffer for the duration of the call.
    // The doorway reads the bytes but never retains or frees them.
    let ptr = nmp_app_dispatch_action_bytes(app, envelope.as_ptr(), envelope.len());
    if ptr.is_null() {
        return CString::new(r#"{"error":"action dispatch returned null"}"#)
            .unwrap_or_else(|_| c"{}".to_owned())
            .into_raw();
    }
    // The kernel returns `{"correlation_id":…}` or `{"error":…}`; echo it back
    // verbatim so the host can free it through `nmp_free_string`. We do NOT
    // parse it here — the host's dispatch helper does (mirroring Chirp's
    // `GroupDiscoveryBridge.dispatchNip29Discovery`).
    ptr
}

/// 29er's app-level chat-send doorway. NOT a NIP-29 action namespace — it is a
/// 29er convenience surface that takes raw user input (`{group, content,
/// mention_pubkeys}`) and is composed + re-emitted under
/// [`PUBLISH_GROUP_EVENT_NAMESPACE`] by [`encode_chat_send_payload`]. Kept as a
/// stable doorway key so both the TUI and the iOS shell route chat sends here.
const CHAT_SEND_NAMESPACE: &str = "nmp.nip29.post_chat_message";

/// The real NIP-29 generic-publish action namespace the composed chat send is
/// dispatched under.
const PUBLISH_GROUP_EVENT_NAMESPACE: &str = "nmp.nip29.publish_group_event";

/// Raw chat-send body the shell hands to [`CHAT_SEND_NAMESPACE`]: the target
/// group, the user's verbatim text (carrying `@<pubkey>` placeholders), and the
/// pubkeys they `@mentioned`. The app composes; the shell holds no NIP-21 / tag
/// knowledge.
#[derive(serde::Deserialize)]
struct ChatSendBody {
    group: GroupId,
    #[serde(default)]
    content: String,
    #[serde(default)]
    mention_pubkeys: Vec<String>,
}

/// Compose a raw chat-send body into the typed [`ActionPayload`] bytes for a
/// kind:9 `PublishGroupEventInput`. NIP-21 mention rewriting + `["p", …]` tags
/// are produced by the shared [`crate::compose::compose_chat_message`] (the one
/// place that composition lives). Returns `None` on a malformed body (D6).
fn encode_chat_send_payload(json: &str) -> Option<Vec<u8>> {
    let body: ChatSendBody = serde_json::from_str(json).ok()?;
    let composed = crate::compose::compose_chat_message(&body.content, &body.mention_pubkeys);
    let input = nmp_nip29::action::PublishGroupEventInput {
        group: body.group,
        kind: KIND_CHAT_MESSAGE,
        content: composed.content,
        tags: composed.tags,
    };
    Some(input.encode())
}

/// Encode `json` into the typed [`ActionPayload`] FlatBuffers bytes for
/// `namespace`. Returns `None` for an unknown namespace (D6 fail-closed — no
/// JSON fallback; the byte doorway has no JSON path). The host builder owns the
/// canonical serde shape; this only re-encodes to typed bytes.
///
/// The set mirrors Chirp's `encode_payload_for_namespace` but strips to the
/// NIP-29 namespaces 29er dispatches. Read/write group chat is first-class:
/// Swift only hands typed JSON bodies across this doorway; Rust/NMP owns
/// event kind, tags, signing, and relay pinning.
fn encode_payload_for_namespace(namespace: &str, json: &str) -> Option<Vec<u8>> {
    use serde::de::DeserializeOwned;
    fn encode<P>(_namespace: &str, json: &str) -> Option<Vec<u8>>
    where
        P: ActionPayload + DeserializeOwned,
    {
        let action: P = serde_json::from_str(json).ok()?;
        Some(action.encode())
    }
    match namespace {
        "nmp.publish" => encode::<nmp_core::publish::PublishAction>(namespace, json),
        "nmp.nip29.discover" => encode::<nmp_nip29::action::DiscoverGroupsInput>(namespace, json),
        "nmp.nip29.join" => encode::<nmp_nip29::action::JoinGroupInput>(namespace, json),
        "nmp.nip29.leave" => encode::<nmp_nip29::action::LeaveGroupInput>(namespace, json),
        "nmp.nip29.create_public_group" => {
            encode::<nmp_nip29::action::CreatePublicGroupInput>(namespace, json)
        }
        "nmp.nip29.put_user" => encode::<nmp_nip29::action::PutUserInput>(namespace, json),
        "nmp.nip29.create_invite" => {
            encode::<nmp_nip29::action::CreateInviteInput>(namespace, json)
        }
        "nmp.nip29.set_parent" => encode::<nmp_nip29::action::SetParentInput>(namespace, json),
        "nmp.nip29.publish_group_event" => {
            encode::<nmp_nip29::action::PublishGroupEventInput>(namespace, json)
        }
        "nmp.nip29.react_in_group" => {
            encode::<nmp_nip29::action::ReactInGroupInput>(namespace, json)
        }
        _ => None,
    }
}

/// Copy a C string into an owned `String`, returning `None` for a null
/// pointer. Mirrors `nmp-app-chirp::ffi::helpers::c_string_opt`.
fn c_string_opt(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    // SAFETY: caller guarantees `ptr` is a valid NUL-terminated C string for
    // the duration of this call (the FFI contract).
    Some(
        unsafe { std::ffi::CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned(),
    )
}
