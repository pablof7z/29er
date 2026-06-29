import Foundation

enum KernelDecodedUpdateFrame {
    case snapshot(KernelUpdateResult)
    case panic(String)
}

/// The decoded snapshot frame. 29er's S01 surface consumes only the NIP-29
/// discovered-groups sidecar + the active-account pubkey + the bare envelope
/// scalars (`rev`, `running`, `lastErrorToast`). New typed slots are added
/// here as 29er grows (mirroring Chirp's `KernelUpdateTypes.swift`).
struct KernelUpdateResult {
    /// Typed `nmp.nip29.discovered_groups` projection decode (`NDGS`). `nil`
    /// ⇒ the sidecar was absent or malformed (the generic JSON `payload`
    /// fallback is not decoded by 29er in S01).
    let typedDiscoveredGroups: DiscoveredGroupsSnapshot?
    /// Typed app-owned `nmp.29er.group_tree` projection decode (`N29T`). `nil`
    /// ⇒ discovery has not been opened or the sidecar was absent/malformed.
    let typedGroupTree: GroupTreeSnapshot?
    /// Typed `nmp.nip29.group_timeline` projection decode (`NGTL`). `nil` ⇒ no
    /// group timeline has been registered or the sidecar was absent/malformed.
    let typedGroupChat: GroupChatSnapshot?
    /// Typed `nmp.nip29.group_roster` projection decode (`NGRS`). `nil` ⇒ no
    /// roster session is open or the sidecar was absent/malformed. Carries the
    /// open group's members (pubkey + roles + admin/member flags) + 39003 role
    /// catalog.
    let typedGroupRoster: GroupRosterSnapshot?
    /// Typed kernel-owned `publish_outbox` projection decode (`KPBO`). `nil` ⇒
    /// no publish outbox sidecar was emitted or the sidecar was malformed.
    let typedPublishOutbox: [PublishOutboxItem]?
    /// Typed `active_account` projection decode (`KACT`). `nil` ⇒ no active
    /// account on this tick.
    let typedActiveAccount: String?
    /// Typed `nmp.nip29.group_defaults` projection decode (`NGDF`). `nil` ⇒ the
    /// sidecar was absent or malformed on this tick. Carries 29er's suggested
    /// public-group host relay URL (Rust-owned operator policy).
    let typedGroupDefaults: GroupDefaultsSnapshot?
    /// Raw typed-projection envelopes from this frame. Keyed row-delta
    /// projections (`refs.profile`) merge through their own host store because
    /// their payload is not one whole value.
    let typedProjections: [TypedProjectionEnvelope]
    /// Typed app-owned `nmp.29er.relay_selector` projection decode (`N29R`).
    /// Rust owns active relay selection and the NIP-51 kind:30002 relay-set
    /// list; Swift renders this snapshot and submits relay intents back.
    let typedRelaySelector: RelaySelectorSnapshot?
    /// Typed kernel-owned `relay_diagnostics` projection decode (`KRDG`).
    /// Carries NIP-11 relay info once NMP has fetched it; Swift renders fields.
    let typedRelayDiagnostics: RelayDiagnosticsSnapshot?
    /// ADR-0044 Tier-3: bare envelope scalars read directly off the
    /// `SnapshotFrame` table. `rev` is the authoritative snapshot revision;
    /// `running` mirrors the kernel's `running` flag; `lastErrorToast` is the
    /// snapshot-driven error toast (nil ⇒ none on this tick).
    let sessionId: UInt64
    let snapshotEpoch: UInt64
    let rev: UInt64
    let running: Bool
    let lastErrorToast: String?
    let lastErrorCategory: String?
    let payloadBytes: Int
    let callbackReceivedAt: ContinuousClock.Instant
    let decodeMicros: Int
}

extension Duration {
    var microseconds: Int {
        let parts = components
        return Int(parts.seconds) * 1_000_000 + Int(parts.attoseconds / 1_000_000_000_000)
    }
}
