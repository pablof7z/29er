//! Bindgen entrypoint for the 29er UniFFI surface.
//!
//! Regenerate Swift (iOS) bindings (macOS):
//!   cargo build -p nmp-app-29er
//!   cargo run -p nmp-app-29er --features bindgen --bin uniffi-bindgen -- \
//!       generate --library target/debug/libnmp_app_29er.dylib \
//!       --language swift --out-dir ios/29er/29er/Generated/
//!
//! Kotlin (Android) swaps `--language kotlin --out-dir <android dir>`.
fn main() {
    uniffi::uniffi_bindgen_main()
}
