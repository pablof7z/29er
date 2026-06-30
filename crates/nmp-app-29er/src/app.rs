//! `TwentyNinerApp` — the 29er UniFFI facade object (#2494).
//!
//! Replaces the deleted hand-written C-ABI (`nmp-ffi` + this crate's old
//! `#[no_mangle] extern "C"` surface in `ffi.rs`). 29er owns its own
//! `nmp-native-runtime` app (composed in [`TwentyNinerApp::new`] via
//! [`crate::composition::compose_29er_runtime`]) and exposes its lifecycle
//! through UniFFI, so the iOS shell consumes generated Swift instead of a
//! hand-maintained header.
//!
//! Why a 29er-owned object (and not the stock `nmp-uniffi::NmpApp`): NIP-29 is
//! not part of the canonical composition `nmp-uniffi` exposes, and a
//! `uniffi::Object` cannot be extended across crates. 29er therefore owns the
//! composition root and the runtime, mirroring `nmp-uniffi`'s lifecycle/
//! dispatch surface.
//!
//! ## Scope (PR-1 — the spine)
//!
//! This object exposes lifecycle + the generic byte-dispatch passthrough
//! only. NIP-29 group-discovery / group-chat typed read sessions and the
//! `dispatchNip29Action` convenience verb are explicitly deferred:
//!
//! * Group-events / discovery / joined-groups typed read sessions are PR-2.
//! * The `dispatchNip29Action` Swift-callable verb (and the relay-selector /
//!   chat-send convenience methods) are PR-3. The underlying encoder
//!   ([`crate::dispatch::dispatch_nip29_action`]) is ported now as a plain
//!   Rust function — not yet exported via `#[uniffi::export]` — so the native
//!   Rust TUI keeps working against the new pin; PR-3 wires the same function
//!   onto this facade for Swift.

use std::sync::Arc;

use nmp_native_runtime::{new_app, NmpApp};

// ── Typed dispatch outcome ───────────────────────────────────────────────────

/// Typed outcome of a dispatch. Exactly one of `correlation_id` (accepted) or
/// `error` (rejected/failed) is `Some`; `code` is present only for coded
/// rejections.
///
/// Facade-local by necessity: UniFFI resolves every exported record to its
/// owning facade namespace, so this crate cannot re-export
/// `nmp_uniffi_support::DispatchOutcome` (or `nmp_uniffi`'s own copy) directly
/// — see docs/builder-guide/15-codegen-and-ffi.md "Why facade-local records".
#[derive(uniffi::Record, Debug, Clone)]
pub struct DispatchOutcome {
    pub correlation_id: Option<String>,
    pub error: Option<String>,
    pub code: Option<String>,
}

impl From<nmp_uniffi_support::DispatchOutcome> for DispatchOutcome {
    fn from(out: nmp_uniffi_support::DispatchOutcome) -> Self {
        DispatchOutcome {
            correlation_id: out.correlation_id,
            error: out.error,
            code: out.code,
        }
    }
}

impl DispatchOutcome {
    pub(crate) fn error(message: impl Into<String>) -> Self {
        DispatchOutcome {
            correlation_id: None,
            error: Some(message.into()),
            code: None,
        }
    }
}

// ── Update sink callback interface ───────────────────────────────────────────

/// Rust→shell push interface: receives NMPU FlatBuffers update frames.
///
/// Implementations MUST NOT call back into any [`TwentyNinerApp`] method from
/// within `on_update`; the runtime quiescence gate would deadlock.
#[uniffi::export(callback_interface)]
pub trait UpdateSink: Send + Sync {
    fn on_update(&self, frame: Vec<u8>);
}

// ── The object ───────────────────────────────────────────────────────────────

/// Arc-wrapped 29er native runtime.
#[derive(uniffi::Object)]
pub struct TwentyNinerApp {
    inner: NmpApp,
}

#[uniffi::export]
impl TwentyNinerApp {
    /// Construct + compose 29er. No IO; the actor is NOT started. Call
    /// configuration setters then [`Self::start`].
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        let mut inner = new_app();
        crate::composition::compose_29er_runtime(&mut inner);
        Arc::new(Self { inner })
    }

    /// Set the LMDB storage directory (pre-start). Empty clears it. Returns
    /// `true` when accepted (`NmpConfigStatus::Ok`).
    pub fn set_storage_path(&self, path: String) -> bool {
        let arg = if path.is_empty() { None } else { Some(path) };
        matches!(
            self.inner.set_storage_path(arg),
            nmp_native_runtime::NmpConfigStatus::Ok
        )
    }

    /// Declare that 29er consumes every kernel-owned built-in Tier-2
    /// projection (full client). Pre-start; idempotent. Replaces the deleted
    /// C-ABI symbol `nmp_app_consume_all_builtin_projections` /
    /// `nmp_app_29er_declare_consumed_projections`.
    pub fn declare_consumed_projections(&self) {
        self.inner.consume_all_builtin_projections();
    }

    /// ADR-0055 Rung 3 — declare that 29er's runtime owns the NMP cache-merge
    /// layer (D3-3) so the kernel may omit `Unchanged` projections from the
    /// frame. Single-writer; call before [`Self::start`]. `true` on success
    /// (or idempotent re-call); `false` if called after start / the registry
    /// is unavailable (informational — the kernel then emits full rows).
    pub fn declare_incremental_apply(&self) -> bool {
        self.inner.declare_incremental_apply().is_ok()
    }

    /// Start the runtime actor. Clamp parity with `nmp-uniffi`: `visible_limit
    /// == 0` → default; else clamp(1..=500). `emit_hz == 0` → default; else
    /// clamp(1..=12).
    pub fn start(&self, visible_limit: u32, emit_hz: u32) {
        nmp_uniffi_support::start_runtime(&self.inner, visible_limit, emit_hz);
    }

    /// Reconfigure rendering limits without restarting (same clamps as `start`).
    pub fn configure(&self, visible_limit: u32, emit_hz: u32) {
        nmp_uniffi_support::configure_runtime(&self.inner, visible_limit, emit_hz);
    }

    /// Pause event processing (no data loss).
    pub fn stop(&self) {
        self.inner.stop_runtime();
    }

    /// Reset transient kernel state.
    pub fn reset(&self) {
        self.inner.reset_runtime();
    }

    /// Idempotent teardown: clears the sink, sends Shutdown, joins threads.
    pub fn shutdown(&self) {
        self.inner.shutdown();
    }

    /// Actor-liveness probe (ADR-0028). `true` while the actor thread runs.
    pub fn is_alive(&self) -> bool {
        self.inner.is_alive()
    }

    /// Report iOS scenePhase = `.active`. Fire-and-forget.
    pub fn lifecycle_foreground(&self) {
        self.inner.lifecycle_foreground();
    }

    /// Report iOS scenePhase = `.background`. Fire-and-forget.
    pub fn lifecycle_background(&self) {
        self.inner.lifecycle_background();
    }

    /// Register (or clear) the NMPU frame observer. After return the previous
    /// sink is neither registered nor mid-invocation (quiescence). Mirrors
    /// `nmp-uniffi::NmpApp::set_update_sink`.
    pub fn set_update_sink(&self, sink: Option<Box<dyn UpdateSink>>) {
        nmp_uniffi_support::set_update_sink(&self.inner, sink, |sink, frame| {
            sink.on_update(frame);
        });
    }

    /// Sign in with a local nsec and (when `make_active`) activate it. The
    /// nsec is wiped on drop (`Zeroizing`). D004: handed to NMP once.
    pub fn signin_nsec(&self, nsec: String, make_active: bool) {
        self.inner.add_signer(
            nmp_core::SignerSource::LocalNsec(zeroize::Zeroizing::new(nsec)),
            make_active,
        );
    }

    /// Remove an identity; the actor owns the active-account transition.
    pub fn remove_account(&self, identity_id: String) {
        self.inner.remove_account(identity_id);
    }

    /// Add a relay. `role` is an NMP relay-role token (e.g. `"both"`).
    pub fn add_relay(&self, url: String, role: String) {
        self.inner.add_relay(url, role);
    }

    /// Remove a relay.
    pub fn remove_relay(&self, url: String) {
        self.inner.remove_relay(url);
    }

    /// Retry a parked publish-outbox row by its handle.
    pub fn retry_publish(&self, handle: String) {
        self.inner.retry_publish(handle);
    }

    /// Seed 29er's Rust-owned default relay set (D7). `true` when ≥1 relay was
    /// handed to the kernel.
    pub fn seed_default_relays(&self) -> bool {
        crate::relay_seeding::seed_default_relays(&self.inner)
    }

    /// Seed relays from a `[["url","role"],…]` JSON array (the `NMP_TEST_RELAYS`
    /// override). `false` on malformed/empty so the caller falls back.
    pub fn seed_relays_from_json(&self, json: String) -> bool {
        crate::relay_seeding::seed_relays_from_json_str(&self.inner, &json)
    }

    /// Dispatch a pre-built `DispatchEnvelope` (the generic byte lane,
    /// ADR-0071). This is the one dispatch verb this PR exposes on the
    /// facade; the richer per-namespace NIP-29 convenience
    /// ([`crate::dispatch::dispatch_nip29_action`]) is PR-3's job to wire in.
    pub fn dispatch_action(&self, envelope: Vec<u8>) -> DispatchOutcome {
        nmp_uniffi_support::dispatch_action_vec(&self.inner, envelope).into()
    }
}

// ── Internal (non-exported) helpers ──────────────────────────────────────────

impl TwentyNinerApp {
    /// The owned `nmp-native-runtime` app. Crate-internal accessor so sibling
    /// modules ([`crate::capability`]) reach the runtime without making
    /// `inner` a public field.
    pub(crate) fn app(&self) -> &NmpApp {
        &self.inner
    }
}
