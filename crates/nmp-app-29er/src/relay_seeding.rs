//! 29er relay-bootstrap seeding.
//!
//! Seeding policy lives in Rust, not in the shell (D7 / thin-shell). Wraps a
//! dependency-free config module ([`crate::config`]) so the default relay set
//! has ONE source of truth.
//!
//! Two entry points, called by [`crate::TwentyNinerApp`] and the native Rust
//! TUI:
//!
//! * [`seed_default_relays`] — production path: add 29er's reference relay
//!   set ([`crate::config::default_relay_bootstrap`]).
//! * [`seed_relays_from_json_str`] — test-override path (`NMP_TEST_RELAYS`):
//!   parse a `[["url","role"],…]` JSON array and add each entry.
//!
//! Both are D6 fire-and-forget: malformed JSON or an empty array degrades to
//! a `false` return rather than raising across the FFI. The Swift shell reads
//! the `NMP_TEST_RELAYS` env var (env plumbing stays in Swift) and calls the
//! JSON path; on a `false` return it falls back to the default path.

use nmp_native_runtime::NmpApp;

/// Seed 29er's reference relay set onto `app`.
///
/// Returns `true` when at least one relay was handed to the kernel. The
/// canonical set comes from [`crate::config::default_relay_bootstrap`].
/// `add_relay` dedups against any session-restored relay rows, so re-seeding
/// an existing install is a no-op on the kernel side.
pub fn seed_default_relays(app: &NmpApp) -> bool {
    let mut seeded = false;
    for entry in crate::config::default_relay_bootstrap() {
        app.add_relay(entry.url.to_string(), entry.role.to_string());
        seeded = true;
    }
    seeded
}

/// Seed relays from a `[["url","role"],…]` JSON array (the `NMP_TEST_RELAYS`
/// override shape).
///
/// Returns `true` when the JSON was well-formed and at least one entry was
/// seeded, `false` when the JSON is malformed or the array is empty — the
/// caller must fall back to [`seed_default_relays`] on `false`.
pub fn seed_relays_from_json_str(app: &NmpApp, json: &str) -> bool {
    let Ok(parsed) = serde_json::from_str::<Vec<[String; 2]>>(json) else {
        return false;
    };
    if parsed.is_empty() {
        return false;
    }
    for entry in &parsed {
        app.add_relay(entry[0].clone(), entry[1].clone());
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_seed_empty_array_returns_false() {
        let app = nmp_native_runtime::new_app();
        assert!(!seed_relays_from_json_str(&app, "[]"));
    }

    #[test]
    fn json_seed_malformed_returns_false() {
        let app = nmp_native_runtime::new_app();
        assert!(!seed_relays_from_json_str(&app, "not json"));
        assert!(!seed_relays_from_json_str(&app, "{}"));
        assert!(!seed_relays_from_json_str(&app, "[[]]"));
    }

    #[test]
    fn default_seed_adds_at_least_one_relay() {
        let app = nmp_native_runtime::new_app();
        assert!(seed_default_relays(&app));
    }

    #[test]
    fn json_seed_well_formed_returns_true() {
        let app = nmp_native_runtime::new_app();
        assert!(seed_relays_from_json_str(
            &app,
            r#"[["wss://x","both"]]"#
        ));
    }
}
