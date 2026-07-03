//! 29er-owned event kinds.
//!
//! NIP-29 is kind-blind transport (it owns only `h`-tag routing + its own
//! 9000-series moderation / 39000-series metadata artifacts — see
//! `nmp_nip29::kinds::KindClass::GroupEvent` doc: "NIP-29 owns no constant
//! for foreign kinds"). Chat is just kind:9, one event kind among many; the
//! *content* and the kind-specific *tags* are the app's concern
//! ([`crate::compose`]), and the *kind number* is the app's concern too.
pub const KIND_CHAT_MESSAGE: u32 = 9;
/// NIP-29 thread/discussion root (deleted from `nmp_nip29::kinds` in the
/// kind-blind-transport migration — same rationale as `KIND_CHAT_MESSAGE`).
pub const KIND_DISCUSSION_OR_ARTIFACT: u32 = 11;
/// 29er-owned ephemeral typing indicator kind.
///
/// `nmp-chat` owns the reusable `["typing", "started"|"stopped"]` tag
/// contract and projection semantics. 29er owns this app-level kind number and
/// publishes it through the generic NIP-29 group-event doorway, keeping
/// `nmp-nip29` kind-blind.
pub const KIND_TYPING_INDICATOR: u32 = 24_010;
