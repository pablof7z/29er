//! 29er relay-bootstrap seeding (D7 — seeding policy lives in Rust, not the
//! shell).
//!
//! Pure helpers over a live [`NmpApp`]; the seeding *verbs* are exposed through
//! [`crate::TwentyNinerApp::seed_default_relays`] /
//! [`crate::TwentyNinerApp::seed_relays_from_json`]. Both wrap the
//! dependency-free [`crate::config`] module so the default relay set has ONE
//! source of truth shared with the NIP-29 group-defaults projection.
//!
//! `add_relay` dedups against session-restored rows, so re-seeding an existing
//! install is a no-op on the kernel side.

use nmp_native_runtime::NmpApp;

/// Seed 29er's reference relay set onto `app`. `true` when ≥1 relay was handed
/// to the kernel (the canonical set is non-empty, so always `true`).
pub fn seed_default_relays(app: &NmpApp) -> bool {
    let mut seeded = false;
    for entry in crate::config::default_relay_bootstrap() {
        app.add_relay(entry.url.to_string(), entry.role.to_string());
        seeded = true;
    }
    seeded
}

/// Seed relays from a `[["url","role"],…]` JSON array (the `NMP_TEST_RELAYS`
/// override shape). `false` on malformed JSON or an empty array — the caller
/// falls back to [`seed_default_relays`].
pub fn seed_relays_from_json_str(app: &NmpApp, json: &str) -> bool {
    let Ok(parsed) = serde_json::from_str::<Vec<[String; 2]>>(json) else {
        return false;
    };
    if parsed.is_empty() {
        return false;
    }
    let mut seeded = false;
    for entry in &parsed {
        app.add_relay(entry[0].clone(), entry[1].clone());
        seeded = true;
    }
    seeded
}

/// First relay URL in a `[["url","role"],…]` JSON array (the `NMP_TEST_RELAYS`
/// override shape), or `None` on malformed / empty input. Used to pin the
/// active NIP-29 relay selection to the test seam so a test session targets the
/// seeded relay instead of restoring the production NIP-51 nip29 relay set.
#[must_use]
pub fn first_relay_url_from_json_str(json: &str) -> Option<String> {
    let parsed = serde_json::from_str::<Vec<[String; 2]>>(json).ok()?;
    parsed.into_iter().next().map(|entry| {
        let [url, _role] = entry;
        url
    })
}

#[cfg(test)]
mod tests {
    /// Parse-only mirror of [`super::seed_relays_from_json_str`]'s reject
    /// conditions, so the malformed/empty contract is unit-tested without a
    /// live runtime app.
    fn parse_only(json: &str) -> bool {
        match serde_json::from_str::<Vec<[String; 2]>>(json) {
            Ok(parsed) => !parsed.is_empty(),
            Err(_) => false,
        }
    }

    #[test]
    fn json_seed_malformed_or_empty_returns_false() {
        assert!(!parse_only("[]"));
        assert!(!parse_only("not json"));
        assert!(!parse_only("{}"));
        assert!(!parse_only("[[]]"));
        assert!(parse_only(r#"[["wss://x","both"]]"#));
    }
}
