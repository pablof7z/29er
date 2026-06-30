//! Native keyring capability sink — the request→response round-trip the iOS
//! shell needs before [`crate::TwentyNinerApp::start`] (Keychain sign-in
//! restore reads from the keyring during actor startup).
//!
//! Uses `nmp-uniffi-support`'s shared capability-callback mechanics so this
//! app-owned UniFFI facade does not duplicate the stock NMP callback bridge —
//! it only defines the facade-local callback-interface trait UniFFI's
//! namespace model requires (see
//! docs/builder-guide/15-codegen-and-ffi.md "Why facade-local records").

use crate::app::TwentyNinerApp;

/// Rust→shell capability round-trip: the kernel calls this to route a
/// `CapabilityRequest` JSON to the platform (iOS Keychain) and expects a
/// `CapabilityEnvelope` JSON back.
#[uniffi::export(callback_interface)]
pub trait CapabilitySink: Send + Sync {
    fn on_capability_request(&self, request_json: String) -> String;
}

#[uniffi::export]
impl TwentyNinerApp {
    /// Register (or clear) the native keyring capability handler. Must be
    /// called before [`TwentyNinerApp::start`] so the handler is in place for
    /// the identity-restore capability requests the actor issues at startup
    /// (Keychain sign-in restore). Pass `None` to clear.
    ///
    /// After this returns, the previous sink is neither registered nor
    /// mid-invocation (the same quiescence contract as `set_update_sink`).
    /// Re-entrancy is forbidden: calling this from inside
    /// `on_capability_request` deadlocks the gate.
    pub fn set_capability_callback(&self, sink: Option<Box<dyn CapabilitySink>>) {
        nmp_uniffi_support::set_capability_callback(self.app(), sink, |sink, request_json| {
            sink.on_capability_request(request_json)
        });
    }
}
