import Combine
import Foundation
import SwiftUI
import os.log

private let kmLog = Logger(subsystem: "io.f7z.app29er.bridge", category: "KernelModel")

/// S02 root-routing state. Derived from the `active_account` typed projection
/// (`KACT`) on every snapshot tick, plus the local submit/load transitions
/// driven by `submitNsec` and the boot timeout.
///
/// `unknown` is the pre-first-snapshot state (and the loading state set by
/// `submitNsec` while waiting for the nsec to be validated and the
/// `active_account` slot to flip). It is also the state the `RootView`
/// falls back to during kernel boot; a 3-second timeout in `RootView`
/// collapses a stuck `unknown` to `signedOut` so the user is never stuck on
/// a spinner.
enum IdentityState: Equatable {
    case unknown       // before first snapshot or loading
    case signedOut     // no active account
    case signedIn(pubkey: String)
    case invalidKey    // nsec was rejected
    case storageError  // keychain/auth error
}

/// `ObservableObject` mirror of the kernel snapshot. The Rust actor pushes
/// binary FlatBuffers updates via the callback; the bridge decodes them and
/// this class republishes the resulting model for SwiftUI consumption.
///
/// 29er's S01 surface holds only the NIP-29 discovered-groups sidecar + the
/// active-account pubkey. New typed slots are added here as 29er grows
/// (mirroring Chirp's `KernelModel.swift`).
@MainActor
final class KernelModel: ObservableObject {

    // ── Typed projection slots — single source of truth for kernel-driven state ──

    /// Typed `nmp.nip29.discovered_groups` sidecar (`NDGS`). `nil` ⇒ the
    /// discovery projection has not been registered (no
    /// `openGroupDiscovery` call yet) or the sidecar was absent/malformed on
    /// the last tick. Read through the `discoveredGroups` accessor.
    @Published var typedDiscoveredGroups: DiscoveredGroupsSnapshot?

    /// Typed `active_account` sidecar (`KACT`). `nil` ⇒ no active account on
    /// the last tick (startup before sign-in). Read through the
    /// `activeAccountPubkey` accessor.
    @Published var typedActiveAccount: String?

    // ── Identity routing (S02) ─────────────────────────────────────────────

    /// Root-routing state derived from `typedActiveAccount` on every tick,
    /// plus the local submit/load transitions driven by `submitNsec`. See
    /// `IdentityState` for the state machine.
    @Published var identityState: IdentityState = .unknown

    // ── Local mutable state ──────────────────────────────────────────────

    @Published var snapshotCount: UInt64 = 0
    @Published var lastSnapshotAt: Date?
    /// Snapshot-driven AND user-clearable error toast. Rust owns the code; the
    /// shell owns the prose.
    @Published var lastErrorToast: String?
    @Published var lastErrorCategory: String?
    @Published var visibleLimit: UInt32 = 80
    @Published var emitHz: UInt32 = 4

    /// D7 actor-death surface — flips to `true` exactly once when the Rust
    /// supervisor emits an `{"t":"panic",...}` update frame (the actor thread
    /// died inside `catch_unwind`) OR when the foreground-resume probe
    /// (`nmp_app_is_alive`, ADR-0028) reports the actor as not running. Set
    /// once, never cleared in-process.
    @Published var kernelIsDead: Bool = false

    // ── Stores & capabilities (non-published) ────────────────────────────

    let kernel = KernelHandle()
    /// Re-entrance guard for `start()`. The snapshot-driven `isRunning`
    /// accessor only flips after the first tick lands, so a re-entrant
    /// `start()` before then would dispatch the FFI twice.
    var startedKernel = false

    /// NIP-29 group-discovery + join mirror — the read side of the discover
    /// screen. Lazy AND relay-keyed: registration deferred until the user
    /// enters a relay URL and taps "Search" (the store's `searchGroups` is the
    /// trigger). Until then the snapshot key is unwired and the store stays
    /// empty. Touching it every tick keeps `apply` symmetric.
    private(set) lazy var discoveredGroups = DiscoveredGroupsStore(kernel: kernel)

    init() {
        if let v = ProcessInfo.processInfo.environment["NMP_VISIBLE_LIMIT"].flatMap(UInt32.init) {
            visibleLimit = v
        }
        if let v = ProcessInfo.processInfo.environment["NMP_EMIT_HZ"].flatMap(UInt32.init) {
            emitHz = v
        }
        kernel.listen({ [weak self] result in
            DispatchQueue.main.async { [weak self] in
                guard let self else { return }
                MainActor.assumeIsolated { self.apply(result: result) }
            }
        }, onPanic: { [weak self] in
            // D7 actor-death — the C callback runs on the Rust listener
            // thread; bounce onto the main runloop so the @Published flip
            // happens on the actor (@MainActor). The Rust supervisor only
            // emits the panic frame once, but `markKernelDead` is idempotent
            // (a stuck-at-true latch) so a stray re-invoke is safe.
            DispatchQueue.main.async { [weak self] in
                guard let self else { return }
                MainActor.assumeIsolated { self.markKernelDead() }
            }
        })
    }
}