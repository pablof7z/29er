import FlatBuffers
import Foundation

/// HAND-WRITTEN glue between the `flatc --swift` FlatBuffers reader structs
/// and the 29er domain types, for the typed-projection-sidecar decode path.
///
/// Mirrors Chirp's `TypedProjectionGlue.swift` but strips to the S01 surface:
/// only the `nmp.nip29.discovered_groups` (`NDGS`) and `active_account`
/// (`KACT`) sidecars. New projection keys are added here as 29er grows.
///
/// Raw protocol values only (D11 — no display helpers). Each function takes
/// the generated reader struct and returns the SAME 29er domain value the
/// generic JSON `payload` path would yield for that key.
enum TypedProjectionGlue {
    // MARK: active_account → String?

    /// Map the typed `active_account` sidecar (`KACT` /
    /// `nmp_kernel_ActiveAccountSnapshot`) to the `String?` the JSON
    /// `projections.active_account` path yields — `nil` when no account is
    /// active (`has_active_account == false` mirrors JSON `null`).
    static func activeAccount(_ reader: nmp_kernel_ActiveAccountSnapshot) -> String? {
        reader.hasActiveAccount ? (reader.pubkey ?? "") : nil
    }

    // MARK: nmp.nip29.discovered_groups → DiscoveredGroupsSnapshot

    /// Map the typed `nmp.nip29.discovered_groups` sidecar (`NDGS` /
    /// `nmp_nip29_DiscoveredGroupsSnapshot`) to the `DiscoveredGroupsSnapshot`
    /// the JSON `projections["nmp.nip29.discovered_groups"]` path yields. Flat
    /// field-for-field copy: a top-level `hostRelayUrl` plus one ordered
    /// `[DiscoveredGroup]` vector (alphabetical by `groupId`; Rust owns the
    /// order). `name`/`picture`/`about` are tag-derived `Option<String>` on
    /// the wire — bare FlatBuffers strings where absent decodes to `nil`;
    /// the glue preserves that `nil` (NOT `?? ""`) so the typed value is
    /// byte-identical to the JSON path's `null`.
    static func discoveredGroups(
        _ reader: nmp_nip29_DiscoveredGroupsSnapshot
    ) -> DiscoveredGroupsSnapshot {
        DiscoveredGroupsSnapshot(
            hostRelayUrl: reader.hostRelayUrl ?? "",
            groups: reader.groups.map { row in
                DiscoveredGroup(
                    groupId: row.groupId ?? "",
                    hostRelayUrl: row.hostRelayUrl ?? "",
                    name: row.name,
                    picture: row.picture,
                    about: row.about,
                    memberCount: row.memberCount,
                    adminCount: row.adminCount,
                    public: row.public_,
                    open: row.open_
                )
            }
        )
    }
}