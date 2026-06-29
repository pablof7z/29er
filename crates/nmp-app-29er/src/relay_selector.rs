use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use nmp_core::actor::{ActorCommand, PublishCommand};
use nmp_core::substrate::{KernelEvent, ObservedProjection, ObservedProjectionRegistrar};
use nmp_core::{canonical_relay_url, ObservedProjectionSink, TypedProjectionData};
use nmp_signer_iface::UnsignedEvent;

#[path = "wire/generated/relay_selector_generated.rs"]
mod generated;

use generated::nmp_app_29er as fb;

pub const RELAY_SELECTOR_KEY: &str = "nmp.29er.relay_selector";
pub const RELAY_SELECTOR_SCHEMA_ID: &str = "nmp.29er.relay_selector";
pub const RELAY_SELECTOR_SCHEMA_VERSION: u32 = 1;
pub const RELAY_SELECTOR_FILE_IDENTIFIER: &[u8; 4] = b"N29R";

const KIND_NIP51_RELAY_SET: u32 = 30_002;
const NIP29_RELAY_SET_IDENTIFIER: &str = "nip29";
const RELAY_LIST_TITLE: &str = "NIP-29 relays";
const RELAY_LIST_DESCRIPTION: &str = "Relays 29er uses for NIP-29 group discovery.";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelaySelectorRow {
    pub relay_url: String,
    pub selected: bool,
    pub from_nip51: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelaySelectorSnapshot {
    pub active_relay_url: String,
    pub relays: Vec<RelaySelectorRow>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RelaySetEvent {
    created_at: u64,
    relays: Vec<String>,
}

pub struct RelaySelectorProjection {
    active_pubkey: nmp_core::slots::ActiveAccountSlot,
    selected_relay: Mutex<String>,
    relay_sets: Mutex<BTreeMap<String, RelaySetEvent>>,
    fallback_relay: String,
}

impl RelaySelectorProjection {
    #[must_use]
    pub fn new(active_pubkey: nmp_core::slots::ActiveAccountSlot, fallback_relay: String) -> Self {
        let selected = canonical_relay_url(&fallback_relay).unwrap_or(fallback_relay.clone());
        Self {
            active_pubkey,
            selected_relay: Mutex::new(selected),
            relay_sets: Mutex::new(BTreeMap::new()),
            fallback_relay: canonical_relay_url(&fallback_relay).unwrap_or(fallback_relay),
        }
    }

    #[must_use]
    pub fn snapshot(&self) -> RelaySelectorSnapshot {
        let active_pubkey = self.active_pubkey();
        let selected = self
            .selected_relay
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_else(|_| self.fallback_relay.clone());
        let list_relays = active_pubkey
            .as_deref()
            .and_then(|pubkey| self.relay_set_for(pubkey));
        let from_nip51 = list_relays
            .as_ref()
            .is_some_and(|relays| !relays.is_empty());
        let relays = list_relays
            .filter(|relays| !relays.is_empty())
            .unwrap_or_else(|| vec![self.fallback_relay.clone()]);

        let active = if relays.iter().any(|relay| relay == &selected) {
            selected
        } else {
            relays
                .first()
                .cloned()
                .unwrap_or_else(|| self.fallback_relay.clone())
        };
        RelaySelectorSnapshot {
            active_relay_url: active.clone(),
            relays: relays
                .into_iter()
                .map(|relay_url| RelaySelectorRow {
                    selected: relay_url == active,
                    relay_url,
                    from_nip51,
                })
                .collect(),
        }
    }

    pub fn select_relay(&self, relay_url: &str) -> Option<String> {
        let canonical = canonical_relay_url(relay_url)?;
        if let Ok(mut selected) = self.selected_relay.lock() {
            *selected = canonical.clone();
        }
        Some(canonical)
    }

    pub fn add_relay(&self, relay_url: &str, tx: &nmp_core::CommandSender) -> Option<String> {
        let canonical = self.select_relay(relay_url)?;
        let active_pubkey = self.active_pubkey()?;
        let mut relays = self
            .relay_set_for(&active_pubkey)
            .unwrap_or_else(|| vec![self.fallback_relay.clone()]);
        if !relays.iter().any(|relay| relay == &canonical) {
            relays.push(canonical.clone());
        }
        self.set_relay_set_for(&active_pubkey, relays.clone());
        publish_relay_set(tx, relays);
        Some(canonical)
    }

    pub fn remove_relay(&self, relay_url: &str, tx: &nmp_core::CommandSender) -> Option<String> {
        let canonical = canonical_relay_url(relay_url)?;
        let active_pubkey = self.active_pubkey()?;
        let mut relays = self
            .relay_set_for(&active_pubkey)
            .unwrap_or_else(|| vec![self.fallback_relay.clone()]);
        relays.retain(|relay| relay != &canonical);
        if relays.is_empty() {
            relays.push(self.fallback_relay.clone());
        }
        if self
            .selected_relay
            .lock()
            .map(|selected| selected.as_str() == canonical)
            .unwrap_or(false)
        {
            if let Some(next) = relays.first().cloned() {
                let _ = self.select_relay(&next);
            }
        }
        self.set_relay_set_for(&active_pubkey, relays.clone());
        publish_relay_set(tx, relays);
        Some(canonical)
    }

    fn active_pubkey(&self) -> Option<String> {
        self.active_pubkey.lock().ok().and_then(|slot| slot.clone())
    }

    fn relay_set_for(&self, pubkey: &str) -> Option<Vec<String>> {
        self.relay_sets
            .lock()
            .ok()
            .and_then(|sets| sets.get(pubkey).map(|event| event.relays.clone()))
    }

    fn set_relay_set_for(&self, pubkey: &str, relays: Vec<String>) {
        let Ok(mut sets) = self.relay_sets.lock() else {
            return;
        };
        let created_at = sets
            .get(pubkey)
            .map(|prior| prior.created_at.saturating_add(1))
            .unwrap_or(0);
        sets.insert(pubkey.to_string(), RelaySetEvent { created_at, relays });
    }
}

impl ObservedProjectionSink for RelaySelectorProjection {
    fn on_kernel_event(&self, event: &KernelEvent) {
        if event.kind != KIND_NIP51_RELAY_SET {
            return;
        }
        if d_tag(event) != Some(NIP29_RELAY_SET_IDENTIFIER) {
            return;
        }
        let relays = relay_tags(event);
        let Ok(mut sets) = self.relay_sets.lock() else {
            return;
        };
        if sets
            .get(&event.author)
            .is_some_and(|prior| event.created_at < prior.created_at)
        {
            return;
        }
        sets.insert(
            event.author.clone(),
            RelaySetEvent {
                created_at: event.created_at,
                relays,
            },
        );
    }
}

pub fn register_relay_selector_runtime(
    app: &nmp_native_runtime::NmpApp,
) -> Arc<RelaySelectorProjection> {
    let projection = Arc::new(RelaySelectorProjection::new(
        app.active_account_handle(),
        crate::config::public_group_relay_url().to_string(),
    ));

    // Register as an ObservedProjectionSink (scope=0 = ActiveAccount — NMP re-routes
    // the subscription on account switch, replacing the old RelaySelectorRuntimeController
    // tick-based subscription management).
    let filter_json = format!(r#"{{"kinds":[{KIND_NIP51_RELAY_SET}]}}"#);
    let replay_shapes: Vec<nmp_planner::InterestShape> =
        nmp_planner::InterestShape::from_filter_json(&filter_json)
            .into_iter()
            .collect();
    let _ = app.open_observed_projection(ObservedProjection {
        observer: Arc::clone(&projection) as Arc<dyn ObservedProjectionSink>,
        filter_json,
        consumer_id: "29er.relay_selector.kind30002".to_string(),
        scope: 0, // ActiveAccount — subscription is re-routed on account switch
        relay_pin: None,
        replay_shapes,
        replay_limit: 20,
    });

    let projection_for_snapshot = Arc::clone(&projection);
    app.register_typed_snapshot_projection(RELAY_SELECTOR_KEY, move || {
        Some(TypedProjectionData {
            key: RELAY_SELECTOR_KEY.to_string(),
            schema_id: RELAY_SELECTOR_SCHEMA_ID.to_string(),
            schema_version: RELAY_SELECTOR_SCHEMA_VERSION,
            file_identifier: String::from_utf8_lossy(RELAY_SELECTOR_FILE_IDENTIFIER).into_owned(),
            payload: encode_relay_selector_snapshot(&projection_for_snapshot.snapshot()),
            ..Default::default()
        })
    });

    projection
}

#[must_use]
pub fn encode_relay_selector_snapshot(snapshot: &RelaySelectorSnapshot) -> Vec<u8> {
    let mut fbb = flatbuffers::FlatBufferBuilder::new();
    let row_offsets: Vec<_> = snapshot
        .relays
        .iter()
        .map(|row| {
            let relay_url = fbb.create_string(&row.relay_url);
            fb::RelaySelectorRow::create(
                &mut fbb,
                &fb::RelaySelectorRowArgs {
                    relay_url: Some(relay_url),
                    selected: row.selected,
                    from_nip51: row.from_nip51,
                },
            )
        })
        .collect();
    let rows = fbb.create_vector(&row_offsets);
    let active_relay_url = fbb.create_string(&snapshot.active_relay_url);
    let root = fb::RelaySelectorSnapshot::create(
        &mut fbb,
        &fb::RelaySelectorSnapshotArgs {
            schema_version: RELAY_SELECTOR_SCHEMA_VERSION,
            active_relay_url: Some(active_relay_url),
            relays: Some(rows),
        },
    );
    fb::finish_relay_selector_snapshot_buffer(&mut fbb, root);
    fbb.finished_data().to_vec()
}

fn publish_relay_set(tx: &nmp_core::CommandSender, relays: Vec<String>) {
    let relays = dedupe_canonical_relays(relays);
    let event = UnsignedEvent {
        pubkey: String::new(),
        kind: KIND_NIP51_RELAY_SET,
        tags: relay_set_tags(&relays),
        content: String::new(),
        created_at: 0,
    };
    let _ = tx.send(ActorCommand::Publish(PublishCommand::UnsignedEvent {
        event,
        correlation_id: Some(crate::app::mint_correlation_id()),
        signer_pubkey: None,
    }));
}

fn relay_set_tags(relays: &[String]) -> Vec<Vec<String>> {
    let mut tags = vec![
        vec!["d".to_string(), NIP29_RELAY_SET_IDENTIFIER.to_string()],
        vec!["title".to_string(), RELAY_LIST_TITLE.to_string()],
        vec![
            "description".to_string(),
            RELAY_LIST_DESCRIPTION.to_string(),
        ],
    ];
    tags.extend(
        relays
            .iter()
            .map(|relay| vec!["relay".to_string(), relay.clone()]),
    );
    tags
}

fn dedupe_canonical_relays(relays: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    relays
        .into_iter()
        .filter_map(|relay| canonical_relay_url(&relay))
        .filter(|relay| seen.insert(relay.clone()))
        .collect()
}

fn d_tag(event: &KernelEvent) -> Option<&str> {
    event.tags.iter().find_map(|tag| match tag.as_slice() {
        [kind, value, ..] if kind == "d" => Some(value.as_str()),
        _ => None,
    })
}

fn relay_tags(event: &KernelEvent) -> Vec<String> {
    dedupe_canonical_relays(
        event
            .tags
            .iter()
            .filter_map(|tag| match tag.as_slice() {
                [kind, relay, ..] if kind == "relay" => Some(relay.clone()),
                _ => None,
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_uses_fallback_until_active_account_relay_set_arrives() {
        let active = Arc::new(Mutex::new(None));
        let projection = RelaySelectorProjection::new(active, "wss://NIP29.F7Z.IO/".to_string());
        assert_eq!(projection.snapshot().active_relay_url, "wss://nip29.f7z.io");
        assert!(!projection.snapshot().relays[0].from_nip51);
    }

    #[test]
    fn projection_reads_active_account_kind_30002_relay_set() {
        let active = Arc::new(Mutex::new(Some("pubkey".to_string())));
        let projection =
            RelaySelectorProjection::new(Arc::clone(&active), "wss://nip29.f7z.io".to_string());
        projection.on_kernel_event(&KernelEvent {
            id: "event".to_string(),
            author: "pubkey".to_string(),
            kind: KIND_NIP51_RELAY_SET,
            created_at: 10,
            tags: vec![
                vec!["d".to_string(), NIP29_RELAY_SET_IDENTIFIER.to_string()],
                vec!["relay".to_string(), "wss://Example.COM/".to_string()],
            ],
            content: String::new(),
            relay_provenance: Vec::new(),
        });
        let snapshot = projection.snapshot();
        assert_eq!(snapshot.active_relay_url, "wss://example.com");
        assert!(snapshot.relays[0].from_nip51);
    }

    #[test]
    fn stale_relay_set_event_is_ignored() {
        let active = Arc::new(Mutex::new(Some("pubkey".to_string())));
        let projection = RelaySelectorProjection::new(active, "wss://fallback.example".to_string());
        for (created_at, relay) in [(20, "wss://new.example"), (10, "wss://old.example")] {
            projection.on_kernel_event(&KernelEvent {
                id: created_at.to_string(),
                author: "pubkey".to_string(),
                kind: KIND_NIP51_RELAY_SET,
                created_at,
                tags: vec![
                    vec!["d".to_string(), NIP29_RELAY_SET_IDENTIFIER.to_string()],
                    vec!["relay".to_string(), relay.to_string()],
                ],
                content: String::new(),
                relay_provenance: Vec::new(),
            });
        }
        assert_eq!(projection.snapshot().active_relay_url, "wss://new.example");
    }
}
