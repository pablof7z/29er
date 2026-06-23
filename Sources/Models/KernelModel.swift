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

    // Current group state
    var currentGroupMembers: [Member] = []
    var currentGroupId: String?

    func signIn(with nsec: String) async {
        // Connect to nmp with nsec, store in keychain
    }

    func fetchGroupTree() async {
        isLoadingGroups = true
        // Fetch from nip29.f7z.io
    }

    func selectGroup(_ group: GroupNode) {
        selectedGroup = group
        currentGroupId = group.id
        // Subscribe to kind:9 for this group
    }

    func postMessage(_ text: String, mentions: [Member]) async {
        // Post kind:9 to current group with p-tags for mentions
    }
}

struct GroupNode: Identifiable, Hashable {
    let id: String
    let name: String
    let children: [GroupNode]
    var isExpanded: Bool = false

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
