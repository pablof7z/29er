//! Typed profile/event reference adapters (ADR-0063 Lane D), re-exposed on
//! [`TwentyNinerApp`].
//!
//! These are the fixed-shape adapters the shell needs for registry components:
//! feed-avatar profile claims (`resolve_profile_ref` / `release_profile_ref`),
//! open-profile cards (`resolve_profile_card_live`), and embedded-event cards
//! (`resolve_event_embed*` / `release_event_ref`). Each fixes
//! namespace/shape/liveness so the shell hand-decodes nothing; the resolved
//! rows return in the keyed `refs.profile` / `refs.event` projections. Mirrors
//! `nmp-uniffi`'s `refs::profile` / `refs::embed` typed adapters one-for-one.
//!
//! `consumer_id` IS the subscription handle: the matching `release_*` uses the
//! same string. Release is idempotent (D6) and invalid keys are silent no-ops.

use nmp_core::__ffi_internal::is_hex_pubkey;

use crate::app::TwentyNinerApp;

#[uniffi::export]
impl TwentyNinerApp {
    /// Resolve a profile ref (feed-avatar shape, CacheOk liveness). Use for
    /// feed-row avatars. D6: invalid `key` is a silent no-op; fire-and-forget.
    pub fn resolve_profile_ref(&self, key: String, consumer_id: String) {
        if !is_hex_pubkey(&key) {
            return;
        }
        self.app().resolve_ref_with_metadata(
            nmp_core::RefNamespace::Profile,
            key,
            consumer_id,
            nmp_core::RefShape::Profile(nmp_core::ProfileShape::Ref),
            nmp_core::RefLiveness::CacheOk,
            nmp_core::RefResolveMetadata::default(),
        );
    }

    /// Resolve a live profile card (full-card shape, Live liveness). Use for
    /// open profile screens. D6: invalid `key` is a silent no-op.
    pub fn resolve_profile_card_live(&self, key: String, consumer_id: String) {
        if !is_hex_pubkey(&key) {
            return;
        }
        self.app().resolve_ref_with_metadata(
            nmp_core::RefNamespace::Profile,
            key,
            consumer_id,
            nmp_core::RefShape::Profile(nmp_core::ProfileShape::Card),
            nmp_core::RefLiveness::Live,
            nmp_core::RefResolveMetadata::default(),
        );
    }

    /// Release a profile ref acquired through a typed profile adapter.
    /// Idempotent (D6).
    pub fn release_profile_ref(&self, key: String, consumer_id: String) {
        if !is_hex_pubkey(&key) {
            return;
        }
        self.app()
            .release_ref(nmp_core::RefNamespace::Profile, key, consumer_id);
    }

    /// Resolve an event embed (embed shape, CacheOk liveness). Use for embedded
    /// `nostr:nevent`/`naddr` cards in rendered content. D6 / fire-and-forget.
    pub fn resolve_event_embed(&self, key: String, consumer_id: String) {
        self.app().resolve_ref_with_metadata(
            nmp_core::RefNamespace::Event,
            key,
            consumer_id,
            nmp_core::RefShape::Event(nmp_core::EventShape::Embed),
            nmp_core::RefLiveness::CacheOk,
            nmp_core::RefResolveMetadata::default(),
        );
    }

    /// Resolve a live event embed (tailing subscription).
    pub fn resolve_event_embed_live(&self, key: String, consumer_id: String) {
        self.app().resolve_ref_with_metadata(
            nmp_core::RefNamespace::Event,
            key,
            consumer_id,
            nmp_core::RefShape::Event(nmp_core::EventShape::Embed),
            nmp_core::RefLiveness::Live,
            nmp_core::RefResolveMetadata::default(),
        );
    }

    /// Release an event ref acquired through a typed event adapter. Idempotent.
    pub fn release_event_ref(&self, key: String, consumer_id: String) {
        self.app()
            .release_ref(nmp_core::RefNamespace::Event, key, consumer_id);
    }
}
