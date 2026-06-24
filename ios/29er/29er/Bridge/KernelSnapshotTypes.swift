import Foundation

/// NIP-29 group identity: the host relay URL plus the in-relay local id.
///
/// Mirrors the Rust `nmp_nip29::GroupId`. The wire JSON is snake_case
/// (`host_relay_url` / `local_id`); Swift call sites use camelCase and the
/// `jsonObject` computed property does the marshalling.
struct GroupId: Hashable, Equatable {
    /// A `wss://` host relay URL.
    let hostRelayUrl: String
    /// The in-relay local id — NIP-29 charset `[a-z0-9-_]+`.
    let localId: String

    /// The exact JSON object shape the Rust `GroupId` deserializes from.
    /// snake_case keys are mandatory — the Rust struct is plain `serde`,
    /// not `.convertFromSnakeCase`-decoded.
    var jsonObject: [String: String] {
        ["host_relay_url": hostRelayUrl, "local_id": localId]
    }
}

/// One discovered NIP-29 group, ready for the discover/join screen to render.
///
/// Raw protocol data only (ADR-0032). Presentation-layer fields such as
/// display-name fallback, avatar initials, and formatted subtitle are computed
/// by the `DiscoveredGroup` extension below.
///
/// No explicit `CodingKeys`: the top-level `.convertFromSnakeCase` strategy
/// maps `"group_id"` / `"host_relay_url"` / `"member_count"` / `"admin_count"`
/// automatically.
///
/// `parent` / `children` are populated from the subgroups PR #2319
/// `parent`/`child` tags on `kind:39000`. Both default to empty/`nil` when the
/// relay has not published them — the S03 group tree walker reconciles the two
/// directions into a forest (see `GroupTree.derive`).
struct DiscoveredGroup: Decodable, Identifiable, Equatable {
    /// The NIP-29 in-relay group id (the `["d", _]` tag value). Stable
    /// list identity inside the discover screen.
    let groupId: String
    /// The host relay this group lives on. NIP-29 identity is the pair
    /// `(host_relay_url, group_id)` — surfaced here so Swift can build a
    /// typed `GroupId` for the join action without re-supplying the URL.
    let hostRelayUrl: String
    let name: String?
    let picture: String?
    let about: String?
    let memberCount: UInt32
    let adminCount: UInt32
    let `public`: Bool
    let open: Bool
    /// `["parent", _]` tag value on the latest 39000, if any. Subgroups
    /// PR #2319: a group without a `parent` tag is a root; the rest are
    /// grouped under the `d` referenced by their `parent` tag.
    let parent: String?
    /// `["child", _]` tag values on the latest 39000. Subgroups PR #2319:
    /// the declared child group ids. Empty until a 39000 carrying `child`
    /// tags arrives.
    let children: [String]

    var id: String { "\(hostRelayUrl)|\(groupId)" }
}

extension DiscoveredGroup {
    /// Display name: `name` when non-empty, `groupId` as fallback (ADR-0032).
    var displayName: String {
        if let n = name, !n.isEmpty { return n }
        return groupId
    }
}

/// The serialised read-model the discover screen consumes. `groups` is ordered
/// alphabetically by `groupId` by the Rust projection — Swift does not
/// re-sort.
struct DiscoveredGroupsSnapshot: Decodable, Equatable {
    /// The host relay this snapshot describes — every row's `hostRelayUrl`
    /// equals this value (the projection is single-relay scoped).
    let hostRelayUrl: String
    let groups: [DiscoveredGroup]

    static let empty = DiscoveredGroupsSnapshot(hostRelayUrl: "", groups: [])
}