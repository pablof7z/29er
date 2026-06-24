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
    let isBranch: Bool
    let lastMessageId: String?
    let lastMessagePubkey: String?
    let lastMessagePreview: String?
    let lastMessageCreatedAt: UInt64
    let unreadCount: UInt32

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
