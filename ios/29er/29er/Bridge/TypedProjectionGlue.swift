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

    // MARK: nmp.nip29.group_events → GroupChatSnapshot

    static func groupEvents(_ reader: nmp_nip29_GroupEventsSnapshot) -> GroupChatSnapshot {
        GroupChatSnapshot(
            messages: reader.events.map { row in
                GroupChatMessage(
                    id: row.id ?? "",
                    pubkey: row.pubkey ?? "",
                    content: row.content ?? "",
                    createdAt: row.createdAt,
                    kind: row.kind
                )
            }
        )
    }

    // MARK: nmp.nip29.group_roster -> GroupRosterSnapshot

    static func groupRoster(_ reader: nmp_nip29_GroupRosterSnapshot) -> GroupRosterSnapshot {
        GroupRosterSnapshot(
            hostRelayUrl: reader.hostRelayUrl ?? "",
            groupId: reader.groupId,
            members: reader.members.map { row in
                GroupRosterMember(
                    pubkey: row.pubkey ?? "",
                    roles: row.roles.map { $0 ?? "" },
                    isAdmin: row.isAdmin,
                    isMember: row.isMember
                )
            },
            roles: reader.roles.map { role in
                GroupRole(name: role.name ?? "", description: role.description)
            }
        )
    }

    // MARK: publish_outbox → [PublishOutboxItem]

    static func publishOutbox(_ reader: nmp_kernel_PublishOutboxSnapshot) -> [PublishOutboxItem] {
        reader.items.map { item in
            PublishOutboxItem(
                handle: item.handle ?? "",
                eventId: item.eventId ?? "",
                kind: item.kind,
                content: item.content ?? "",
                createdAt: item.createdAt,
                status: item.status ?? "",
                canRetry: item.canRetry,
                targetRelays: Int(item.targetRelays),
                relays: item.relays.map { relay in
                    PublishOutboxRelay(
                        relayUrl: relay.relayUrl ?? "",
                        status: relay.status ?? "",
                        attempt: relay.attempt,
                        message: relay.message ?? "",
                        relayReason: relay.relayReason ?? ""
                    )
                }
            )
        }
    }

    // MARK: nmp.nip29.discovered_groups → DiscoveredGroupsSnapshot

    /// Map the typed `nmp.nip29.discovered_groups` sidecar (`NDGS` /
    /// `nmp_nip29_DiscoveredGroupsSnapshot`) to the `DiscoveredGroupsSnapshot`
    /// the JSON `projections["nmp.nip29.discovered_groups"]` path yields. Flat
    /// field-for-field copy: a top-level `hostRelayUrl` plus one ordered
    /// `[DiscoveredGroup]` vector (alphabetical by `groupId`; Rust owns the
    /// order). `name`/`picture`/`about`/`parent` are tag-derived
    /// `Option<String>` on the wire — bare FlatBuffers strings where absent
    /// decodes to `nil`; the glue preserves that `nil` (NOT `?? ""`) so the
    /// typed value is byte-identical to the JSON path's `null`. `children` is
    /// a FlatBuffers vector of strings — absent decodes to `[]` (matching the
    /// Rust `Vec<String>` default).
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
                    open: row.open_,
                    parent: row.parent,
                    children: row.children.map { $0 ?? "" }
                )
            }
        )
    }

    // MARK: nmp.nip29.group_defaults → GroupDefaultsSnapshot

    /// Map the typed `nmp.nip29.group_defaults` sidecar (`NGDF` /
    /// `nmp_nip29_GroupDefaultsSnapshot`) to the `GroupDefaultsSnapshot` the
    /// JSON `projections["nmp.nip29.group_defaults"]` path yields. Flat
    /// single-field copy: `suggestedRelayUrl` is 29er's app/operator-owned
    /// default host relay URL for a new public group, carried verbatim (raw
    /// protocol value; the shell pre-fills it but the user may overwrite it).
    static func groupDefaults(
        _ reader: nmp_nip29_GroupDefaultsSnapshot
    ) -> GroupDefaultsSnapshot {
        GroupDefaultsSnapshot(suggestedRelayUrl: reader.suggestedRelayUrl ?? "")
    }

    // MARK: nmp.29er.group_tree → GroupTreeSnapshot

    static func groupTree(_ reader: nmp_app_29er_GroupTreeSnapshot) -> GroupTreeSnapshot {
        let nodes = reader.nodes.map(groupTreeNode(_:))
        return GroupTreeSnapshot(
            hostRelayUrl: reader.hostRelayUrl ?? "",
            roots: reader.roots.map(groupTreeNode(_:)),
            allNodes: Dictionary(uniqueKeysWithValues: nodes.map { ($0.groupId, $0) }),
            totalCount: Int(reader.totalCount)
        )
    }

    private static func groupTreeNode(_ row: nmp_app_29er_GroupTreeNode) -> GroupTreeNode {
        GroupTreeNode(
            groupId: row.groupId ?? "",
            hostRelayUrl: row.hostRelayUrl ?? "",
            name: row.name,
            parentId: row.parentId,
            childIds: row.childIds.map { $0 ?? "" },
            memberCount: row.memberCount,
            adminCount: row.adminCount,
            isPublic: row.public_,
            isOpen: row.open_,
            isMember: row.isMember,
            isAdmin: row.isAdmin,
            isBranch: row.branch,
            lastMessageId: row.lastMessageId,
            lastMessagePubkey: row.lastMessagePubkey,
            lastMessagePreview: row.lastMessagePreview,
            lastMessageCreatedAt: row.lastMessageCreatedAt,
            unreadCount: row.unreadCount
        )
    }

    // MARK: nmp.29er.relay_selector → RelaySelectorSnapshot

    static func relaySelector(
        _ reader: nmp_app_29er_RelaySelectorSnapshot
    ) -> RelaySelectorSnapshot {
        RelaySelectorSnapshot(
            activeRelayUrl: reader.activeRelayUrl ?? "",
            relays: reader.relays.map { row in
                RelaySelectorRow(
                    relayUrl: row.relayUrl ?? "",
                    selected: row.selected,
                    fromNip51: row.fromNip51
                )
            }
        )
    }

    // MARK: relay_diagnostics → RelayDiagnosticsSnapshot

    static func relayDiagnostics(
        _ reader: nmp_kernel_RelayDiagnosticsSnapshot
    ) -> RelayDiagnosticsSnapshot {
        RelayDiagnosticsSnapshot(
            relays: reader.relays.map { row in
                let info = row.info
                return RelayDiagnosticsRelay(
                    relayUrl: row.relayUrl ?? "",
                    connection: row.connection ?? "",
                    nip11Name: info?.hasName == true ? info?.name : nil
                )
            }
        )
    }
}
