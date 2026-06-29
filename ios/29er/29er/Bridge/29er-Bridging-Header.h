#ifndef APP29ER_BRIDGING_HEADER_H
#define APP29ER_BRIDGING_HEADER_H

// Clean-break bridging header. The hand-written `NmpCore.h` C-ABI is deleted;
// the 29er shell now consumes the generated UniFFI `TwentyNinerApp` object.
//
// The generated `nmp_app_29er.swift` references the low-level UniFFI C symbols
// (`ffi_nmp_app_29er_*`, `uniffi_nmp_app_29er_*`) declared in the generated FFI
// header. Including that header here exposes those symbols to the whole Swift
// module, so the generated Swift's `#if canImport(nmp_app_29erFFI)` falls
// through to the inline (bridging-header) path — no separate clang module is
// needed. The actual symbol bodies link from the `libnmp_app_29er.a` aggregate
// archive (`-lnmp_app_29er`, see project.yml LIBRARY_SEARCH_PATHS).
#import "../Generated/uniffi/nmp_app_29erFFI.h"

#endif /* APP29ER_BRIDGING_HEADER_H */
