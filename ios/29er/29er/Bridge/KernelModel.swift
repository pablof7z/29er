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

    /// Typed app-owned `nmp.29er.group_tree` sidecar (`N29T`). Rust derives
    /// this from the NIP-29 discovery projection; Swift only renders it.
    @Published var typedGroupTree: GroupTreeSnapshot?

    /// Typed app-owned `nmp.29er.group_chat` sidecar (`N29C`). Rust owns the
    /// selected group's event filter, newest-first ordering, content tree, and
    /// demand lists; Swift renders it.
    @Published var typedGroupChat: GroupChatSnapshot?

    /// Typed `nmp.nip29.group_roster` sidecar (`NGRS`). Rust owns selected
    /// group membership/admin derivation; Swift renders it.
    @Published var typedGroupRoster: GroupRosterSnapshot?

    /// Typed kernel-owned `publish_outbox` sidecar (`KPBO`). Rust owns publish
    /// lifecycle, retry policy, and offline queue state; Swift renders rows.
    @Published var typedPublishOutbox: [PublishOutboxItem]?

    /// Typed `active_account` sidecar (`KACT`). `nil` ⇒ no active account on
    /// the last tick (startup before sign-in). Read through the
    /// `activeAccountPubkey` accessor.
    @Published var typedActiveAccount: String?

    /// Typed `nmp.nip29.group_defaults` sidecar (`NGDF`). `nil` ⇒ the defaults
    /// projection sidecar was absent/malformed on the last tick. Carries 29er's
    /// Rust-owned suggested public-group host relay URL. Read through the
    /// `groupDefaults` accessor.
    @Published var typedGroupDefaults: GroupDefaultsSnapshot?

    /// Monotonic UI invalidation token for `refs.profile` row commits. The
    /// actual rows live in `profileRefs`; this published scalar redraws views
    /// that read the registry `NostrProfileHost` environment.
    @Published var profileRefsRevision: UInt64 = 0

    /// Monotonic UI invalidation token for the whole-value `refs.event.envelopes`
    /// sidecar. The envelope map itself lives in `eventEnvelopes`.
    @Published var eventRefsRevision: UInt64 = 0

    /// Typed app-owned `nmp.29er.relay_selector` sidecar (`N29R`). Rust owns
    /// active relay selection and the NIP-51 kind:30002 relay-set list.
    @Published var typedRelaySelector: RelaySelectorSnapshot?

    /// Typed kernel-owned `relay_diagnostics` sidecar (`KRDG`). NMP owns the
    /// NIP-11 fetch; Swift only renders the relay info fields.
    @Published var typedRelayDiagnostics: RelayDiagnosticsSnapshot?

    // ── Identity routing (S02) ─────────────────────────────────────────────

    /// Root-routing state derived from `typedActiveAccount` on every tick,
    /// plus the local submit/load transitions driven by `submitNsec`. See
    /// `IdentityState` for the state machine.
    @Published var identityState: IdentityState = .unknown

    // ── Group-tree selection (S03) ────────────────────────────────────────

    /// The currently selected group id in the group-tree navigation, or
    /// `nil` when nothing is selected. Selection is UI state only; group tree
    /// data comes from the Rust `nmp.29er.group_tree` projection.
    @Published var selectedGroupId: String?

    /// Set the selected group id. Called from `GroupTreeRow`'s
    /// `NavigationLink` tap. Idempotent — setting the same id twice is a
    /// no-op for SwiftUI's diffing.
    func selectGroup(_ groupId: String) {
        selectedGroupId = groupId
    }

    func openGroupEvents(_ groupId: String) {
        selectedGroupId = groupId
        discoveredGroups.markGroupRead(groupId: groupId)
        guard let node = groupTree.allNodes[groupId] else { return }
        let group = GroupId(hostRelayUrl: node.hostRelayUrl, localId: node.groupId)
        kernel.registerGroupChat(groupId: group)
        kernel.openGroupRoster(groupId: group)
    }

    @discardableResult
    func selectNip29Relay(_ relayUrl: String) -> Bool {
        kernel.selectNip29Relay(relayUrl)
    }

    @discardableResult
    func addNip29Relay(_ relayUrl: String) -> Bool {
        kernel.addNip29Relay(relayUrl)
    }

    @discardableResult
    func removeNip29Relay(_ relayUrl: String) -> Bool {
        kernel.removeNip29Relay(relayUrl)
    }

    // ── Local mutable state ──────────────────────────────────────────────

    @Published var snapshotCount: UInt64 = 0
    @Published var lastSnapshotAt: Date?
    /// Snapshot-driven AND user-clearable error toast. Rust owns the code; the
    /// shell owns the prose.
    @Published var lastErrorToast: String?
    @Published var lastErrorCategory: String?
    @Published var visibleLimit: UInt32 = 80
    @Published var emitHz: UInt32 = 4

    /// D7 actor-death surface: flips to `true` exactly once when the Rust
    /// supervisor emits an `{"t":"panic",...}` update frame (the actor thread
    /// died inside `catch_unwind`) OR when the foreground-resume probe
    /// reports the actor as not running. Set
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

    let profileRefs = ProfileRefStore()
    let eventEnvelopes = EventEnvelopeStore()

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
