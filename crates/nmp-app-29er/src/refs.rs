//! Event reference adapters exposed on the 29er facade.
//!
//! Registry components stay on typed ref doors instead of generic shell-side
//! substrate calls.

use crate::app::TwentyNinerApp;

#[uniffi::export]
impl TwentyNinerApp {
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

    pub fn release_event_ref(&self, key: String, consumer_id: String) {
        self.app()
            .release_ref(nmp_core::RefNamespace::Event, key, consumer_id);
    }
}
