//! Bindgen entrypoint for the 29er UniFFI facade surface.
//!
//! Regenerate Swift (iOS) bindings:
//!   cargo build -p nmp-app-29er
//!   cargo run -p nmp-app-29er --features bindgen --bin uniffi-bindgen -- \
//!       generate --library target/debug/libnmp_app_29er.dylib \
//!       --language swift --out-dir ios/29er/29er/Bridge/Generated/
//!
//! Mirrors NMP's own `crates/nmp-uniffi/src/bin/uniffi_bindgen.rs` /
//! `ci/check-uniffi-bindings-drift.sh` invocation pattern.
fn main() {
    uniffi::uniffi_bindgen_main()
}
