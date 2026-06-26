//! 29er relay-bootstrap seeding over the C-ABI.
//!
//! Seeding policy lives in Rust, not in the Swift shell (D7 / thin-shell). This
//! mirrors `nmp-app-chirp::ffi::relay_seeding`: both wrap a dependency-free
//! config module ([`crate::config`]) so the default relay set has ONE source of
//! truth. Before this module existed the iOS shell
//! (`KernelModel+Lifecycle.swift`) hardcoded `wss://nip29.f7z.io` + the `"both"`
//! role, which could drift from the relay surfaced by the NIP-29
//! group-defaults projection.
//!
//! Two entry points, mirroring Chirp:
//!
//! * [`nmp_app_29er_seed_default_relays`] — production path: add 29er's
//!   reference relay set ([`crate::config::default_relay_bootstrap`]).
//! * [`nmp_app_29er_seed_relays_from_json`] — test-override path
//!   (`NMP_TEST_RELAYS`): parse a `[["url","role"],…]` JSON array and add each
//!   entry.
//!
//! Both are D6 fire-and-forget: a null app, malformed JSON, or an empty array
//! degrades to a `false` return rather than raising across the FFI. The Swift
//! shell reads the `NMP_TEST_RELAYS` env var (env plumbing stays in Swift) and
//! calls the JSON path; on a `false` return it falls back to the default path.

use std::ffi::{c_char, CStr};

use nmp_ffi::{nmp_app_add_relay, NmpApp};

/// Seed 29er's reference relay set onto `app`.
///
/// Returns `true` when at least one relay was handed to the kernel, `false`
/// when `app` is null (D6). The canonical set comes from
/// [`crate::config::default_relay_bootstrap`]. `nmp_app_add_relay` dedups
/// against any session-restored relay rows, so re-seeding an existing install
/// is a no-op on the kernel side.
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_29er_seed_default_relays(app: *mut NmpApp) -> bool {
    if app.is_null() {
        return false;
    }
    let mut seeded = false;
    for entry in crate::config::default_relay_bootstrap() {
        let (Ok(url), Ok(role)) = (
            std::ffi::CString::new(entry.url),
            std::ffi::CString::new(entry.role),
        ) else {
            continue;
        };
        nmp_app_add_relay(app, url.as_ptr(), role.as_ptr());
        seeded = true;
    }
    seeded
}

/// Seed relays from a `[["url","role"],…]` JSON array (the `NMP_TEST_RELAYS`
/// override shape).
///
/// Returns `true` when the JSON was well-formed and at least one entry was
/// seeded, `false` when `app`/`json` is null, the JSON is malformed, or the
/// array is empty — the Swift caller must fall back to
/// [`nmp_app_29er_seed_default_relays`] on `false`.
///
/// # Safety
///
/// `json` must be a valid nul-terminated C string or null.
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_29er_seed_relays_from_json(
    app: *mut NmpApp,
    json: *const c_char,
) -> bool {
    if json.is_null() {
        return false;
    }
    // SAFETY: caller guarantees `json` (non-null, checked above) is a valid
    // nul-terminated C string for the duration of this call.
    let Ok(json) = (unsafe { CStr::from_ptr(json) }).to_str() else {
        return false;
    };
    seed_relays_from_json_str(app, json)
}

/// Parse-and-seed core, split out so unit tests can drive it with a `&str`
/// without manufacturing a C string. Same `true`/`false` contract as the
/// C-ABI wrapper: `false` on a null app, malformed JSON, or an empty array.
fn seed_relays_from_json_str(app: *mut NmpApp, json: &str) -> bool {
    if app.is_null() {
        return false;
    }
    let Ok(parsed) = serde_json::from_str::<Vec<[String; 2]>>(json) else {
        return false;
    };
    if parsed.is_empty() {
        return false;
    }
    let mut seeded = false;
    for entry in &parsed {
        let (Ok(url), Ok(role)) = (
            std::ffi::CString::new(entry[0].as_str()),
            std::ffi::CString::new(entry[1].as_str()),
        ) else {
            continue;
        };
        nmp_app_add_relay(app, url.as_ptr(), role.as_ptr());
        seeded = true;
    }
    seeded
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr;

    fn null_app() -> *mut NmpApp {
        ptr::null_mut()
    }

    #[test]
    fn default_seed_on_null_app_returns_false() {
        assert!(!nmp_app_29er_seed_default_relays(null_app()));
    }

    #[test]
    fn json_seed_on_null_app_returns_false() {
        assert!(!seed_relays_from_json_str(
            null_app(),
            r#"[["wss://x","both"]]"#
        ));
    }

    #[test]
    fn json_seed_empty_array_returns_false() {
        assert!(!seed_relays_from_json_str(null_app(), "[]"));
    }

    #[test]
    fn json_seed_malformed_returns_false() {
        assert!(!seed_relays_from_json_str(null_app(), "not json"));
        assert!(!seed_relays_from_json_str(null_app(), "{}"));
        assert!(!seed_relays_from_json_str(null_app(), "[[]]"));
    }
}
