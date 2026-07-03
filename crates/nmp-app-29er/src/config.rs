//! Shared 29er app configuration.
//!
//! Mirrors `nmp-chirp-config`: the single source of truth for 29er's default
//! relay set + the suggested NIP-29 public-group host relay. Keeping the URLs
//! here (Rust) — not in the Swift/Kotlin shell — upholds the thin-shell
//! doctrine (D7): the shell never hardcodes a relay URL or a relay role. The
//! same constants feed relay seeding, relay selection, and TUI login defaults,
//! so a new public group's host relay and the bootstrap relay can never drift.

/// One `{url, role}` bootstrap relay entry. `role` is an NMP relay-role token
/// (e.g. `"both"` / `"indexer"`), handed verbatim to the app's relay seeding
/// path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TwentyNinerRelayBootstrapEntry {
    pub url: &'static str,
    pub role: &'static str,
}

/// 29er's bootstrap NIP-29 relay (R002 / M001 product decision). Also the
/// suggested host relay for new public groups.
pub const NIP29_RELAY_URL: &str = "wss://nip29.f7z.io";

/// The relays seeded onto a fresh 29er install. NIP-29 group traffic (read +
/// write) flows through `NIP29_RELAY_URL`, so it carries the `"both"` role.
pub const RELAY_BOOTSTRAP: &[TwentyNinerRelayBootstrapEntry] = &[TwentyNinerRelayBootstrapEntry {
    url: NIP29_RELAY_URL,
    role: "both",
}];

/// 29er's operator-selected default Blossom upload server.
///
/// Native shells read this from Rust and may let users override it per upload.
/// The value is not a framework default: it is 29er product policy, just like
/// `NIP29_RELAY_URL`.
pub const BLOSSOM_UPLOAD_SERVERS: &[&str] = &["https://blossom.primal.net"];

/// The canonical 29er relay bootstrap set. Startup seeding iterates this; the
/// kernel dedups against session-restored rows, so re-seeding an existing
/// install is a no-op.
#[must_use]
pub fn default_relay_bootstrap() -> &'static [TwentyNinerRelayBootstrapEntry] {
    RELAY_BOOTSTRAP
}

/// 29er's suggested NIP-29 public-group host relay.
///
/// This is 29er operator policy. Native shells may read it through the app
/// facade/config path, but must not hardcode the URL independently.
#[must_use]
pub fn public_group_relay_url() -> &'static str {
    NIP29_RELAY_URL
}

#[must_use]
pub fn default_blossom_upload_servers() -> &'static [&'static str] {
    BLOSSOM_UPLOAD_SERVERS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_is_non_empty_and_well_formed() {
        let bootstrap = default_relay_bootstrap();
        assert!(!bootstrap.is_empty(), "29er must ship ≥1 default relay");
        for entry in bootstrap {
            assert!(!entry.url.is_empty(), "relay URL must not be empty");
            assert!(!entry.role.is_empty(), "relay role must not be empty");
        }
    }

    #[test]
    fn public_group_relay_is_in_bootstrap() {
        // The suggested host relay must be one we actually seed, so a fresh
        // install can read+write its own newly-created public group.
        assert!(default_relay_bootstrap()
            .iter()
            .any(|e| e.url == public_group_relay_url()));
    }

    #[test]
    fn blossom_upload_servers_are_http_urls() {
        for server in default_blossom_upload_servers() {
            assert!(
                server.starts_with("https://") || server.starts_with("http://"),
                "Blossom upload server must be HTTP(S): {server}"
            );
        }
    }
}
