//! The `pub extern "C"` registration + NIP-29 group-discovery entry points
//! the 29er iOS shell links against. Mirrors `nmp-app-chirp::ffi` but strips
//! to the S01 surface: register, open/close group discovery, register group
//! chat, dispatch action bytes, declare consumed projections, unregister.

use std::collections::BTreeMap;
use std::ffi::c_char;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use nmp_core::dispatch_envelope::{encode_dispatch_envelope, DISPATCH_ENVELOPE_SCHEMA_VERSION};
use nmp_core::substrate::{ActionPayload, ViewDependencies};
use nmp_core::{KernelEventObserver, KernelEventObserverId, TypedProjectionData};
use nmp_ffi::{nmp_app_dispatch_action_bytes, NmpApp};
use nmp_nip29::group_id::GroupId;
use nmp_nip29::interest::host_pinned_interest;
use nmp_nip29::kinds::{KIND_CHAT_MESSAGE, KIND_DISCUSSION_OR_ARTIFACT};
use nmp_nip29::projection::DiscoveredGroupsProjection;
use nmp_nip29::register::{wire_group_chat, wire_group_members};
use nmp_planner::stable_hash::stable_hash64;
use nmp_planner::{InterestId, InterestLifecycle, InterestScope};

use crate::group_tree::{
    encode_group_tree_snapshot, GroupTreeProjection, GROUP_TREE_FILE_IDENTIFIER,
    GROUP_TREE_SCHEMA_ID, GROUP_TREE_SCHEMA_VERSION,
};

const GROUP_CHAT_HISTORY_LIMIT: u32 = 200;

/// Opaque handle returned by [`nmp_app_29er_register`] and consumed by
/// [`nmp_app_29er_unregister`]. Boxed on the heap; the pointer is opaque to C.
pub struct TwentyNinerHandle {
    app: *mut NmpApp,
}

pub struct TwentyNinerGroupDiscoveryHandle {
    observer_ids: Vec<KernelEventObserverId>,
    tree_projection: Arc<GroupTreeProjection>,
    members_projection: Arc<nmp_nip29::projection::GroupMembersProjection>,
    teardown_fn: Box<dyn FnOnce(Vec<KernelEventObserverId>) + Send>,
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
    let _ = nmp_nip29::register::register_actions(unsafe { &mut *app });

    // Wire the NIP-29 group-create defaults projection so the suggested
    // public-group relay URL surfaces under `"nmp.nip29.group_defaults"`.
    // Output-only: the projection observes no kernel events — its snapshot is
    // a pure function of the `nmp-nip29` crate constant — so this is a one-time
    // registration at app init (same pattern as Chirp's `wire_group_defaults`).
    //
    // SAFETY: shared-ref borrow; the projection registration is internally
    // lock-guarded.
    nmp_nip29::register::wire_group_defaults(unsafe { &*app });

    // D6 — guard the write-through before allocating the handle. A null
    // `handle_out` is a programmer-error contract violation; returning
    // `NullApp` (the same discriminant Chirp uses for this case) is the safe,
    // D6-compliant behaviour.
    if handle_out.is_null() {
        return NmpRegisterStatus::NullApp as u32;
    }
    let handle = Box::into_raw(Box::new(TwentyNinerHandle { app }));
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

/// Wire a NIP-29 `GroupChatProjection` for a single group into `app`.
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

    wire_group_chat(app_ref, group_id.clone());
    app_ref.push_interest(group_chat_interest(&group_id));
}

fn group_chat_interest(group_id: &GroupId) -> nmp_planner::LogicalInterest {
    let id = stable_hash64((
        "29er.nip29.group_chat",
        group_id.host_relay_url.as_str(),
        group_id.local_id.as_str(),
    ));
    let mut interest = host_pinned_interest(
        id,
        group_id,
        [KIND_CHAT_MESSAGE, KIND_DISCUSSION_OR_ARTIFACT],
        BTreeMap::new(),
        InterestLifecycle::Tailing,
    );
    interest.shape.limit = Some(GROUP_CHAT_HISTORY_LIMIT);
    interest
}

fn group_tree_chat_summary_interest(host_relay_url: &str) -> nmp_planner::LogicalInterest {
    let id = stable_hash64(("29er.nip29.group_tree.chat_summary", host_relay_url));
    ViewDependencies {
        kinds: vec![KIND_CHAT_MESSAGE],
        relay_pin: Some(host_relay_url.to_string()),
        ..Default::default()
    }
    .into_logical_interest(
        InterestId(id),
        InterestScope::Global,
        InterestLifecycle::Tailing,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_chat_interest_is_host_pinned_and_tailing() {
        let group = GroupId::new("wss://groups.example.com", "rust-nostr");
        let interest = group_chat_interest(&group);

        assert_eq!(
            interest.shape.relay_pin.as_deref(),
            Some("wss://groups.example.com")
        );
        assert!(interest.shape.kinds.contains(&KIND_CHAT_MESSAGE));
        assert!(interest.shape.kinds.contains(&KIND_DISCUSSION_OR_ARTIFACT));
        assert!(interest.shape.tags.get("h").unwrap().contains("rust-nostr"));
        assert_eq!(interest.shape.limit, Some(GROUP_CHAT_HISTORY_LIMIT));
        assert_eq!(interest.lifecycle, InterestLifecycle::Tailing);
    }

    #[test]
    fn group_chat_interest_id_is_deterministic_per_group() {
        let a1 = group_chat_interest(&GroupId::new("wss://groups.example.com", "room-a"));
        let a2 = group_chat_interest(&GroupId::new("wss://groups.example.com", "room-a"));
        let b = group_chat_interest(&GroupId::new("wss://groups.example.com", "room-b"));

        assert_eq!(a1.id, a2.id);
        assert_ne!(a1.id, b.id);
    }
}

/// Open a NIP-29 group-discovery session for one host relay.
///
/// The **read side** of the NIP-29 group-discovery flow. Constructs a
/// [`nmp_nip29::projection::DiscoveredGroupsProjection`] scoped to the
/// supplied relay URL, plugs it in as a `KernelEventObserver` (ingest), and
/// registers its snapshot read under `"nmp.nip29.discovered_groups"` (output).
/// Kind:39000/39001/39002 events for that relay then surface on every snapshot
/// tick under that key.
///
/// Returns a heap-allocated opaque handle the caller MUST free via
/// [`nmp_app_29er_close_group_discovery`]. A null `app`, null/non-UTF-8/empty
/// `host_relay_url`, or poisoned observer slot returns NULL (D6).
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

fn open_group_discovery_with_tree(
    app: &NmpApp,
    relay_url: String,
) -> Option<TwentyNinerGroupDiscoveryHandle> {
    let projection = Arc::new(DiscoveredGroupsProjection::new(relay_url.clone()));
    let observer_id =
        app.register_event_observer(Arc::clone(&projection) as Arc<dyn KernelEventObserver>);
    if observer_id.0 == 0 {
        return None;
    }
    let tree_messages = Arc::new(GroupTreeProjection::new());
    let tree_observer_id =
        app.register_event_observer(Arc::clone(&tree_messages) as Arc<dyn KernelEventObserver>);
    if tree_observer_id.0 == 0 {
        app.unregister_event_observer(observer_id);
        return None;
    }
    let Some((members_projection, members_observer_id)) =
        wire_group_members(app, relay_url.clone())
    else {
        app.unregister_event_observer(observer_id);
        app.unregister_event_observer(tree_observer_id);
        return None;
    };

    let discovered_projection = Arc::clone(&projection);
    app.register_typed_snapshot_projection("nmp.nip29.discovered_groups", move || {
        let snapshot = discovered_projection.snapshot();
        Some(TypedProjectionData {
            key: "nmp.nip29.discovered_groups".to_string(),
            schema_id: nmp_nip29::wire::discovered_groups_fb::DISCOVERED_GROUPS_SCHEMA_ID
                .to_string(),
            schema_version: nmp_nip29::wire::discovered_groups_fb::DISCOVERED_GROUPS_SCHEMA_VERSION,
            file_identifier: String::from_utf8_lossy(
                nmp_nip29::wire::discovered_groups_fb::DISCOVERED_GROUPS_FILE_IDENTIFIER,
            )
            .into_owned(),
            payload: nmp_nip29::wire::discovered_groups_fb::encode_discovered_groups_snapshot(
                &snapshot,
            ),
            ..Default::default()
        })
    });

    let tree_projection = Arc::clone(&projection);
    let tree_messages_projection = Arc::clone(&tree_messages);
    app.register_typed_snapshot_projection("nmp.29er.group_tree", move || {
        let snapshot = tree_projection.snapshot();
        let messages = tree_messages_projection.snapshot();
        Some(TypedProjectionData {
            key: "nmp.29er.group_tree".to_string(),
            schema_id: GROUP_TREE_SCHEMA_ID.to_string(),
            schema_version: GROUP_TREE_SCHEMA_VERSION,
            file_identifier: String::from_utf8_lossy(GROUP_TREE_FILE_IDENTIFIER).into_owned(),
            payload: encode_group_tree_snapshot(&snapshot, &messages),
            ..Default::default()
        })
    });

    app.push_interest(group_tree_chat_summary_interest(&relay_url));

    let app_addr = (app as *const NmpApp) as usize;
    Some(TwentyNinerGroupDiscoveryHandle {
        observer_ids: vec![observer_id, tree_observer_id, members_observer_id],
        tree_projection: tree_messages,
        members_projection,
        teardown_fn: Box::new(move |ids| {
            let app = unsafe { &*(app_addr as *const NmpApp) };
            for id in ids {
                app.unregister_event_observer(id);
            }
            app.remove_snapshot_projection("nmp.nip29.discovered_groups");
            app.remove_snapshot_projection("nmp.29er.group_tree");
            app.remove_snapshot_projection("nmp.nip29.group_members");
        }),
    })
}

/// Close a NIP-29 group-discovery session opened by
/// [`nmp_app_29er_open_group_discovery`].
///
/// Unregisters the event observer and removes the
/// `"nmp.nip29.discovered_groups"` typed snapshot projection so no stale group
/// catalog is emitted after the discover screen is dismissed. The handle memory
/// is reclaimed; the pointer MUST NOT be used after this call.
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
    (handle.teardown_fn)(handle.observer_ids);
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

/// Select the group whose 39002 members should be emitted in the
/// `"nmp.nip29.group_members"` typed projection.
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_29er_select_group_members(
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
    handle.members_projection.select_group(group_id);
}

/// Process-local correlation-id source for 29er's byte-doorway dispatches.
///
/// Mirrors `nmp-app-chirp::dispatch_bytes::mint_correlation_id`: a monotone
/// atomic counter satisfies the "unique within one running process for the
/// lifetime of an in-flight operation" contract with zero extra deps. The
/// `29er-` prefix namespaces it so it never collides with the kernel's hex
/// correlation ids.
static NEXT_CORRELATION_ID: AtomicU64 = AtomicU64::new(1);

fn mint_correlation_id() -> String {
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

    let Some(payload) = encode_payload_for_namespace(&ns, &body) else {
        return CString::new(format!(
            r#"{{"error":"no typed payload encoder for action namespace '{ns}'"}}"#
        ))
        .unwrap_or_else(|_| c"{}".to_owned())
        .into_raw();
    };

    let correlation_id = mint_correlation_id();
    let envelope = encode_dispatch_envelope(
        &correlation_id,
        &ns,
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
        "nmp.nip29.post_chat_message" => {
            encode::<nmp_nip29::action::PostChatMessageInput>(namespace, json)
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
