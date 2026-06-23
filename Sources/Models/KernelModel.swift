import SwiftUI

@MainActor
final class KernelModel: ObservableObject {
    @Published var nsec: String = ""
    @Published var groupTree: [GroupNode] = []
    @Published var selectedGroup: GroupNode?
    @Published var messages: [Message] = []
    @Published var isConnected: Bool = false
    @Published var isLoadingGroups: Bool = false
    @Published var error: String?

    var currentGroupMembers: [Member] = []
    var currentGroupId: String?

    private let relay = RelayService()

    func signIn(with nsec: String) async {
        self.nsec = nsec
        await fetchGroupTree()
    }

    func fetchGroupTree() async {
        isLoadingGroups = true
        defer { isLoadingGroups = false }
        do {
            let rawGroups = try await relay.fetchGroups()
            let activity = try await relay.fetchActivity(groupIds: rawGroups.map(\.id))
            groupTree = Self.buildTree(from: rawGroups, activity: activity)
        } catch {
            self.error = error.localizedDescription
        }
    }

    func selectGroup(_ group: GroupNode) {
        selectedGroup = group
        currentGroupId = group.id
    }

    func postMessage(_ text: String, mentions: [Member]) async {
    }

    static func buildTree(from groups: [RawGroup], activity: [String: Date]) -> [GroupNode] {
        var nodes: [String: GroupNode] = [:]
        for g in groups {
            nodes[g.id] = GroupNode(
                id: g.id,
                name: g.name,
                children: [],
                lastActivityAt: activity[g.id] ?? .distantPast
            )
        }
        var childrenByParent: [String?: [String]] = [:]
        for g in groups {
            childrenByParent[g.parent, default: []].append(g.id)
        }
        func build(_ id: String) -> GroupNode {
            var node = nodes[id]!
            let childIds = childrenByParent[id] ?? []
            node.children = childIds
                .map { build($0) }
                .sorted { $0.lastActivityAt > $1.lastActivityAt }
            if !node.children.isEmpty {
                node.lastActivityAt = max(node.lastActivityAt, node.children.map(\.lastActivityAt).max() ?? node.lastActivityAt)
            }
            return node
        }
        let roots = groups.filter { g in
            guard let p = g.parent else { return true }
            return nodes[p] == nil
        }
        return roots
            .map { build($0.id) }
            .sorted { $0.lastActivityAt > $1.lastActivityAt }
    }
}

struct GroupNode: Identifiable, Hashable {
    let id: String
    let name: String
    var children: [GroupNode]
    var lastActivityAt: Date

    var isLeaf: Bool {
        children.isEmpty
    }
}

struct Message: Identifiable {
    let id: String
    let author: String
    let content: String
    let timestamp: Date
    let status: MessageStatus
    let mentions: [String]
}

enum MessageStatus {
    case pending
    case confirmed
    case failed
}

struct Member: Identifiable {
    let id: String
    let pubkey: String
    let name: String?
}
