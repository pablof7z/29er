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

// ─── 29er group-chat read model ───────────────────────────────────────────

struct GroupChatReaction: Equatable {
    let emoji: String
    let count: UInt64
}

/// One rendered group-chat message from the app-owned `app.29er.group_chat`
/// projection. Rust owns filtering, ordering, content enrichment, and demand
/// extraction; Swift only renders this shape.
struct GroupChatMessage: Identifiable, Equatable {
    let id: String
    let pubkey: String
    let rawContent: String
    let copyText: String
    let createdAt: UInt64
    let kind: UInt32
    let contentTree: ContentTreeWire?
    let mentionPubkeys: [String]
    let eventRefUris: [String]
    let eventRefPrimaryIds: [String]
    let reactions: [GroupChatReaction]
    let reactionReactorPubkeys: [String]
}

/// The serialised read model a group chat consumes. `messages` is ordered
/// newest-first by Rust; Swift does not re-sort or tokenize content.
struct GroupChatSnapshot: Equatable {
    let messages: [GroupChatMessage]
    let profileDemandPubkeys: [String]
    let eventRefUris: [String]
    let eventRefPrimaryIds: [String]

    static let empty = GroupChatSnapshot(
        messages: [],
        profileDemandPubkeys: [],
        eventRefUris: [],
        eventRefPrimaryIds: []
    )
}

// ─── NIP-29 selected-group roster read model ──────────────────────────────

/// One roster row for the currently open NIP-29 group. Raw protocol values
/// only; Rust owns membership/admin derivation and Swift renders fallbacks.
struct GroupRosterMember: Identifiable, Equatable {
    let pubkey: String
    let roles: [String]
    let isAdmin: Bool
    let isMember: Bool

    var id: String { pubkey }

    var title: String {
        return pubkey.shortHex
    }

    var roleBadge: String { isAdmin ? "Admin" : "Member" }
}

struct GroupRole: Equatable {
    let name: String
    let description: String?
}

/// Roster for the selected group only. `groupId == nil` means no group has
/// been selected on the Rust projection yet.
struct GroupRosterSnapshot: Equatable {
    let hostRelayUrl: String
    let groupId: String?
    let members: [GroupRosterMember]
    let roles: [GroupRole]

    static let empty = GroupRosterSnapshot(hostRelayUrl: "", groupId: nil, members: [], roles: [])
}

extension String {
    var shortHex: String {
        guard count > 16 else { return self }
        return "\(prefix(8))…\(suffix(8))"
    }
}

// ─── Kernel publish-outbox read model ─────────────────────────────────────

/// One target relay of an in-flight publish. Raw kernel tokens only; Swift
/// renders labels but does not decide retry or delivery state.
struct PublishOutboxRelay: Decodable, Identifiable, Equatable {
    let relayUrl: String
    let status: String
    let attempt: UInt32
    let message: String
    let relayReason: String

    var id: String { relayUrl }
}

/// One in-flight publish row from the kernel-owned publish engine.
///
/// A generic outbox/rendering DTO — it mirrors the canonical kernel
/// `publish_outbox` (`KPBO`) schema field-for-field. `kind` is canonical and
/// retained for rendering, but the shell MUST NOT branch group policy on it.
/// The previously-decoded `tags` field was stale Swift/generated drift (the
/// pinned `publish_outbox.fbs` carries no `tags`, so it always decoded empty);
/// it is dropped. Group-scoped pending state (chat delivery status, membership
/// / admin actions) is owned by Rust projections, never reconstructed here from
/// raw outbox tags.
struct PublishOutboxItem: Decodable, Identifiable, Equatable {
    let handle: String
    let eventId: String
    let kind: UInt32
    let content: String
    let createdAt: UInt64
    let status: String
    let canRetry: Bool
    let targetRelays: Int
    let relays: [PublishOutboxRelay]

    var id: String { handle.isEmpty ? eventId : handle }
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
    /// The relay set this snapshot describes — every row's `hostRelayUrl`
    /// names a member of it (the projection tracks a SET of relays since
    /// NIP-29 multi-relay group discovery, #93). A tracked relay with zero
    /// groups so far is still listed here.
    let hostRelayUrls: [String]
    let groups: [DiscoveredGroup]

    static let empty = DiscoveredGroupsSnapshot(hostRelayUrls: [], groups: [])
}
