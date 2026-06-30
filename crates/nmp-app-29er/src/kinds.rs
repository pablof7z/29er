//! 29er-owned event kinds.
//!
//! NIP-29 is kind-blind transport (it owns only `h`-tag routing + its own
//! 9000-series moderation / 39000-series metadata artifacts — see
//! `nmp_nip29::kinds::KindClass::GroupEvent` doc: "NIP-29 owns no constant
//! for foreign kinds"). Chat is just kind:9, one event kind among many; the
//! *content* and the kind-specific *tags* are the app's concern
//! ([`crate::compose`]), and the *kind number* is the app's concern too.
pub const KIND_CHAT_MESSAGE: u32 = 9;
