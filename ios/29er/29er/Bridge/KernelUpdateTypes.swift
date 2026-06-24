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
    /// Typed `active_account` projection decode (`KACT`). `nil` ⇒ no active
    /// account on this tick.
    let typedActiveAccount: String?
    /// ADR-0044 Tier-3: bare envelope scalars read directly off the
    /// `SnapshotFrame` table. `rev` is the authoritative snapshot revision;
    /// `running` mirrors the kernel's `running` flag; `lastErrorToast` is the
    /// snapshot-driven error toast (nil ⇒ none on this tick).
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
