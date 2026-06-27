#ifndef NMP_CORE_H
#define NMP_CORE_H

#include <stdbool.h>
#include <stdint.h>

// 29er uses the raw C bridge over the NMP kernel actor. This header MUST stay
// in sync with the non-test-gated `#[no_mangle] extern "C" fn nmp_app_*`
// symbols exported from `vendor/nmp/crates/nmp-ffi/src/` and the 29er per-app
// crate `crates/nmp-app-29er/src/ffi.rs`.
//
// The minimal surface 29er's S01 slice links:
//   * nmp_app_new / nmp_app_free                              (lifecycle)
//   * nmp_app_set_update_callback                             (snapshot stream)
//   * nmp_app_set_storage_path / nmp_app_start / nmp_app_stop  (boot)
//   * nmp_app_is_alive                                         (liveness probe)
//   * nmp_app_declare_incremental_apply                        (R3 cache-merge)
//   * nmp_app_signin_nsec                                      (identity)
//   * nmp_app_set_capability_callback                          (keyring socket)
//   * nmp_app_add_relay                                        (bootstrap)
//   * nmp_app_dispatch_action_bytes                            (action doorway)
//   * nmp_app_29er_register / nmp_app_29er_unregister          (composition)
//   * nmp_app_29er_declare_consumed_projections                 (Tier-2 gate)
//   * nmp_app_29er_open_group_discovery                         (NIP-29 read)
//   * nmp_app_29er_close_group_discovery                        (NIP-29 teardown)
//   * nmp_app_29er_register_group_chat                         (NIP-29 chat read)
//   * nmp_app_29er_dispatch_action_bytes                       (NIP-29 discover/join)
//   * nmp_free_string                                          (Rust-heap free)

void *nmp_app_new(void);
void nmp_app_free(void *app);

typedef enum NmpConfigStatus {
    NmpConfigStatus_Ok             = 0,
    NmpConfigStatus_NullApp        = 1,
    NmpConfigStatus_AlreadyStarted = 2,
    NmpConfigStatus_Unavailable    = 3,
} NmpConfigStatus;

// Borrowed FlatBuffers `nmp.transport.UpdateFrame` bytes. The pointer is valid
// only for the callback duration; Swift copies before decoding.
typedef void (*NmpUpdateCallback)(void *context, const uint8_t *bytes, uintptr_t len);
void nmp_app_set_update_callback(void *app, void *context, NmpUpdateCallback callback);

// Persistent storage directory for the LMDB EventStore backend. Must be
// called before `nmp_app_start`; a NULL or empty `path` clears it. Returns
// NmpConfigStatus_AlreadyStarted if called after nmp_app_start.
uint32_t nmp_app_set_storage_path(void *app, const char *path);

void nmp_app_start(void *app, unsigned int visible_limit, unsigned int emit_hz);
void nmp_app_stop(void *app);
void nmp_app_reset(void *app);

// Actor-liveness probe (D7 pull-side sibling of the push-side panic frame).
// Returns `1` when the kernel's actor thread is still running, `0` when it
// has terminated (panic, clean Shutdown, or "never started").
uint8_t nmp_app_is_alive(void *app);

// T118 / G3 — iOS scenePhase → kernel lifecycle bridge. 29er's `@main` App
// observes `@Environment(\.scenePhase)` and reports `.active` / `.background`
// here; the kernel decides what each phase MEANS (D7). Fire-and-forget (D6).
void nmp_app_lifecycle_foreground(void *app);
void nmp_app_lifecycle_background(void *app);

// ADR-0055 Rung 3 — declare that this host's runtime owns the NMP cache-merge
// layer (D3-3) so the kernel may omit `Unchanged` projections from the frame.
// Single-writer, call before `nmp_app_start`.
int nmp_app_declare_incremental_apply(void *app);

// NIP-46 signer broker (Stage 4). Call `nmp_signer_broker_init(app)` exactly
// once, right after `nmp_app_new()`, before `nmp_app_start()`. Returns
// NmpConfigStatus_AlreadyStarted when called too late. 29er does not use
// bunker sign-in in S01, but the broker is part of the canonical NMP
// composition and must be initialised before start.
uint32_t nmp_signer_broker_init(void *app);

// Identity: paste an nsec to sign in and set as the active account.
// make_active=1 signs in and activates; make_active=0 registers a visible
// secondary signer without activating it.
void nmp_app_signin_nsec(void *app, const char *secret, uint8_t make_active);

// Remove an identity. If it is the active account, the kernel clears or
// retargets active_account and enqueues keyring forget work.
void nmp_app_remove_account(void *app, const char *identity_id);

// ── T151 — capability socket ──────────────────────────────────────────────
//
// `nmp_app_set_capability_callback` registers the native handler that the
// kernel calls (synchronously) whenever it needs a platform capability (e.g.
// iOS Keychain via PD-019/T96). The callback receives the
// `CapabilityRequest` JSON and MUST return a freshly heap-allocated
// `CapabilityEnvelope` JSON string; that string MUST then be released by the
// caller via `nmp_free_string`. Passing NULL for `callback` unregisters the
// handler; a request received while unregistered yields an error envelope
// (D6), never a crash.
//
// There is one C callback for every capability; the Swift-side
// `TwentyNinerCapabilities.handleJSON` routes the request to the capability
// owning its `namespace` (keyring). Rust invokes this from the actor thread
// (never the main thread), so a synchronous capability may block here safely.
// The returned C string is heap-allocated via `strdup` so it is compatible
// with Rust's `CString::from_raw` on Apple platforms (both use the system
// malloc allocator).
typedef char *(*NmpCapabilityCallback)(void *context, const char *request_json);
void nmp_app_set_capability_callback(void *app, void *context, NmpCapabilityCallback callback);

// Relay bootstrap. `role` is a NMP relay role token (e.g. "outbox", "inbox").
void nmp_app_add_relay(void *app, const char *url, const char *role);
void nmp_app_remove_relay(void *app, const char *url);

// 29er relay-seeding (D7 — seeding policy lives in Rust, not the shell).
//   * nmp_app_29er_seed_default_relays    — production default relay set.
//   * nmp_app_29er_seed_relays_from_json  — NMP_TEST_RELAYS override
//     (`[["url","role"],…]`); returns false on null/malformed/empty so the
//     caller falls back to the default path.
// Both are fire-and-forget (D6); the kernel dedups against session-restored
// rows so re-seeding an existing install is a no-op.
bool nmp_app_29er_seed_default_relays(void *app);
bool nmp_app_29er_seed_relays_from_json(void *app, const char *json);

// ADR-0064 / Cut-B (#1756) — the typed-FlatBuffers BYTE doorway (sole
// remaining dispatch entry point). The caller passes the bytes of an open
// `DispatchEnvelope` (correlation_id + action_namespace + schema_version +
// opaque per-crate payload). The 29er-specific helper
// `nmp_app_29er_dispatch_action_bytes` builds this envelope in Rust so the
// shell never hand-assembles FlatBuffers. Returns the heap-allocated
// `{"correlation_id":"<id>"}` (accepted+enqueued) or `{"error":"…"}` JSON,
// which MUST be freed via `nmp_free_string`. Fail-closed (D6): a null `app`,
// a null `ptr`, an oversize / malformed envelope, or an unknown namespace all
// return `{"error":…}` — never NULL for a non-NULL app.
char *nmp_app_dispatch_action_bytes(void *app, const uint8_t *ptr, uintptr_t len);

// ── nmp-app-29er per-app FFI ──────────────────────────────────────────────
//
// `libnmp_app_29er.a` is the 29er Rust aggregate archive: doctrine D0 keeps
// protocol/app glue outside nmp-core while still letting the iOS shell link
// one Rust archive.
//
// Flow:
// 1. Call `nmp_app_29er_register(app, &handle)` once after `nmp_app_new()`
//    succeeds. Returns NmpRegisterStatus (0 = Ok). On Ok, `handle` is written
//    with a non-null opaque pointer.
// 2. Call `nmp_app_29er_declare_consumed_projections(app)` before
//    `nmp_app_start` so the kernel narrows Tier-2 built-in output to what
//    29er consumes.
// 3. Open/close group-discovery sessions against host relays; register group
//    chat for a single group; dispatch NIP-29 discover/join actions.
// 4. On teardown, call `nmp_app_29er_unregister(handle)` BEFORE
//    `nmp_app_free(app)`.

typedef enum : uint32_t {
    NmpRegisterStatus_Ok      = 0,
    NmpRegisterStatus_NullApp = 1,
} NmpRegisterStatus29er;

uint32_t nmp_app_29er_register(void *app, void **handle_out);
void nmp_app_29er_unregister(void *handle);

// ADR-0053 — declare that this host consumes every Tier-2 kernel-owned
// built-in projection (the ONE non-footgun way to receive the full set). 29er
// is a full client, so it follows Chirp's posture. Idempotent; call before
// `nmp_app_start`. A null `app` is a silent no-op (D6).
void nmp_app_29er_declare_consumed_projections(void *app);

// ── NIP-29 group-chat read projection ────────────────────────────────────
//
// Wires a single NIP-29 group's chat-message read model into the kernel.
// Pure consumption — the read side of a group-chat screen.
//
//   • `group_id_json` is a JSON object naming the target group:
//       {"host_relay_url":"wss://groups.example.com","local_id":"room"}
//   • Returns void — registers no handle and exports no companion
//     `unregister`. The group's timeline events surface on every kernel
//     snapshot tick under the `projections` key `"nmp.nip29.group_timeline"`.
//   • Fire-and-forget (D6): a null `app`, null / invalid-UTF-8
//     `group_id_json`, or a JSON shape that does not deserialize to a
//     `GroupId` all degrade to a silent no-op.
void nmp_app_29er_register_group_chat(void *app, const char *group_id_json);

// ── NIP-29 group-discovery open/close lifecycle ──────────────────────────
//
// Open a group-discovery session for a single host relay. The session owns
// a `DiscoveredGroupsProjection` for kinds 39000/39001/39002 — the read side
// of a discover/join screen. Tear it down with
// `nmp_app_29er_close_group_discovery` when the screen is dismissed.
//
// `nmp_app_29er_open_group_discovery`:
//   • `host_relay_url` is the relay to discover groups on (`wss://…`).
//   • Returns an opaque `void *` handle on success, NULL on failure (D6).
//   • Discovered groups surface under the `projections` key
//     `"nmp.nip29.discovered_groups"` on every snapshot tick until the
//     session is closed.
//   • `app` MUST outlive the handle. Call
//     `nmp_app_29er_close_group_discovery` before `nmp_app_free`.
//
// `nmp_app_29er_close_group_discovery`:
//   • Unregisters the event observer and removes the
//     `"nmp.nip29.discovered_groups"` snapshot projection so no stale group
//     catalog is emitted after the screen is dismissed.
//   • Reclaims the handle; the pointer MUST NOT be used after this call.
//   • D6: a null `handle` is a silent no-op.
void *nmp_app_29er_open_group_discovery(void *app, const char *host_relay_url);
void nmp_app_29er_close_group_discovery(void *handle);
void nmp_app_29er_mark_group_read(void *handle, const char *group_id);
void nmp_app_29er_select_group_members(void *handle, const char *group_id);

// ADR-0064 / S4 (#1782) — 29er's direct (namespace, body_json) BYTE doorway,
// for the sites that already hold a Rust-shaped body string (NIP-29 group
// ops). Rust converts the verbatim body to the namespace's typed payload
// bytes and dispatches through the byte doorway; only typed bytes cross to
// the kernel. Same `{"correlation_id"}` / `{"error"}` return + free
// contract as `nmp_app_dispatch_action_bytes`. Fail-closed (D6) on
// null/unknown namespace.
char *nmp_app_29er_dispatch_action_bytes(void *app, const char *namespace, const char *body_json);

// Kernel-owned publish lifecycle control plane. `handle` is the opaque
// `publish_outbox` row handle; Rust owns retry policy and no-ops invalid or
// stale handles (D6).
void nmp_app_retry_publish(void *app, const char *handle);

// ADR-0063 typed profile-ref adapters. Registry profile/avatar components claim
// visible pubkeys through these fire-and-forget seams; the resolved kind:0 rows
// return in the keyed `refs.profile` projection.
void nmp_app_resolve_profile_ref(void *app, const char *key, const char *consumer_id);
void nmp_app_release_profile_ref(void *app, const char *key, const char *consumer_id);

// ── nmp-content pure tokenizer (Layer A content renderer) ─────────────────
//
// Tokenize Nostr event content into the FFI-stable `ContentTreeWire` JSON the
// SwiftUI `NostrContentView` renders. Pure function — resolves no entities and
// mutates no kernel state (mentions/embeds resolve via the separate
// `nmp_app_resolve_ref` seam). `tags_json`, when non-NULL, is a JSON
// `[[string]]` event-tag array used for NIP-30 emoji resolution.
//
//   mode: 0 = plain · 1 = markdown · 2 = auto (markdown vs plain by `kind`)
//
// Returns a heap-allocated `{"ok":true,"tree":{…}}` (or
// `{"ok":false,"error":"…"}`) JSON string that MUST be freed via
// `nmp_free_string`. Never returns NULL for valid pointers (D6).
char *nmp_content_tokenize_text(const char *content, const char *tags_json, int mode, uint32_t kind);

// Release a Rust-heap C string returned by ANY NMP FFI function. Null-safe.
// This is the ONLY correct freer — the host's free(3) must NOT be used.
void nmp_free_string(char *ptr);

#endif
