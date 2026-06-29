//! Native keyring capability sink — the request→response round-trip the iOS
//! shell needs first (Keychain sign-in restore reads from the keyring during
//! `start()`).
//!
//! Mirrors `nmp-uniffi`'s `CapabilitySink` / `set_capability_callback` exactly,
//! re-exposed on [`TwentyNinerApp`] because a `uniffi::Object` cannot be
//! extended across crates and 29er cannot depend on `nmp-uniffi` (the
//! `setup_scaffolding!()` symbols would collide). Both paths drive the SAME
//! `nmp-core` `CapabilityCallbackGate` (`set_native_handler`), so the kernel's
//! `in_flight` + `Condvar` quiescence gate protects this path identically.

use std::sync::Arc;

use nmp_core::__ffi_internal::{capability_error_envelope, NativeCapabilityHandler};

use crate::app::TwentyNinerApp;

/// Rust→shell capability round-trip: the kernel calls this to route a
/// `CapabilityRequest` JSON to the platform (iOS Keychain) and expects a
/// `CapabilityEnvelope` JSON back.
///
/// # Contract
///
/// * `request_json` is a pre-copied JSON string — no Rust lock is held during
///   the call. The implementation may block; it MUST NOT call
///   [`TwentyNinerApp::set_capability_callback`] for the same app from inside
///   this method (reentrancy deadlocks the quiescence gate).
/// * The returned string must be a valid `CapabilityEnvelope` JSON
///   (`{"namespace":…,"correlation_id":…,"result_json":…}`). D6: a panic or
///   invalid return is caught and converted to an error envelope.
#[uniffi::export(callback_interface)]
pub trait CapabilitySink: Send + Sync {
    fn on_capability_request(&self, request_json: String) -> String;
}

#[uniffi::export]
impl TwentyNinerApp {
    /// Register (or clear) the native keyring capability handler. Must be called
    /// before [`TwentyNinerApp::start`] so the handler is in place for the
    /// identity-restore capability requests the actor issues at startup
    /// (Keychain sign-in restore). Pass `None` to clear.
    ///
    /// After this returns, the previous sink is neither registered nor
    /// mid-invocation (the same `CapabilityCallbackGate` quiescence contract as
    /// `set_update_sink`). Re-entrancy is forbidden: calling this from inside
    /// `on_capability_request` deadlocks the gate.
    pub fn set_capability_callback(&self, sink: Option<Box<dyn CapabilitySink>>) {
        let handler: Option<NativeCapabilityHandler> = sink.map(|s| {
            let s: Arc<dyn CapabilitySink> = Arc::from(s);
            Arc::new(move |request_json: String| -> String {
                let req_for_call = request_json.clone();
                let s = Arc::clone(&s);
                // D6: a Swift/Kotlin throw must not unwind into the dispatch
                // thread — panics become error envelopes, never crashes.
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
                    s.on_capability_request(req_for_call)
                }));
                result.unwrap_or_else(|_| capability_error_envelope(&request_json, "sink-panicked"))
            }) as NativeCapabilityHandler
        });
        self.app()
            .capability_callback_slot()
            .set_native_handler(handler);
    }
}
