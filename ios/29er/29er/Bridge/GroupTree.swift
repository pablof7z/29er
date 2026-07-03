import Foundation

struct GroupTreeNode: Identifiable, Equatable {
    let groupId: String
    let hostRelayUrl: String
    let name: String?
    let parentId: String?
    let childIds: [String]
    let memberCount: UInt32
    let adminCount: UInt32
    let isPublic: Bool
    let isOpen: Bool
    /// Viewer membership/admin truth for the active account, emitted by the
    /// Rust group-tree projection (sourced from the account-scoped
    /// `JoinedGroupsProjection`). The shell renders these flags directly — it
    /// does NOT scan a member roster to derive membership/admin (D11).
    let isMember: Bool
    let isAdmin: Bool
    let isBranch: Bool
    let lastMessageId: String?
    let lastMessagePubkey: String?
    let lastMessagePreview: String?
    let lastMessageCreatedAt: UInt64
    let unreadCount: UInt32
    let typingCount: UInt32

    var id: String { groupId }
    var displayName: String { name?.isEmpty == false ? name! : groupId }
    var hasLastMessage: Bool { lastMessageId?.isEmpty == false }
}

struct GroupTreeSnapshot: Equatable {
    let hostRelayUrl: String
    let roots: [GroupTreeNode]
    let allNodes: [String: GroupTreeNode]
    let totalCount: Int

    static let empty = GroupTreeSnapshot(hostRelayUrl: "", roots: [], allNodes: [:], totalCount: 0)
}

struct RelaySelectorRow: Identifiable, Equatable {
    let relayUrl: String
    let selected: Bool
    let fromNip51: Bool

    var id: String { relayUrl }
}

struct RelaySelectorSnapshot: Equatable {
    let activeRelayUrl: String
    let relays: [RelaySelectorRow]

    static let empty = RelaySelectorSnapshot(activeRelayUrl: "", relays: [])
}

struct RelayDiagnosticsRelay: Equatable {
    let relayUrl: String
    let connection: String
    let nip11Name: String?
}

struct RelayDiagnosticsSnapshot: Equatable {
    let relays: [RelayDiagnosticsRelay]

    static let empty = RelayDiagnosticsSnapshot(relays: [])

    func relay(for url: String) -> RelayDiagnosticsRelay? {
        relays.first { $0.relayUrl == url }
    }
}

extension String {
    var relayHostLabel: String {
        if let host = URLComponents(string: self)?.host, !host.isEmpty {
            return host
        }
        return replacingOccurrences(of: "wss://", with: "")
            .replacingOccurrences(of: "ws://", with: "")
            .trimmingCharacters(in: CharacterSet(charactersIn: "/"))
    }
}
