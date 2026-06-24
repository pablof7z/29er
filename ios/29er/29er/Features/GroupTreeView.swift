import SwiftUI
import UIKit
import os.log

private let gtLog = Logger(subsystem: "io.f7z.app29er.bridge", category: "GroupTreeView")

/// S03 main group-tree navigation screen. Renders the Rust-owned
/// `GroupTreeSnapshot` as an expandable forest per D009:
///
///   - `NavigationStack` + push navigation (no `.sidebar` split column —
///     iPhone-only).
///   - Branch rows keep the branch's own `NavigationLink` (D003 — branch nodes
///     are real groups; tapping the label navigates to the group's timeline,
///     tapping the chevron expands/collapses children).
///   - Leaf rows are plain chat-list rows.
///   - Pushed destination is a placeholder timeline view (`TimelinePlaceholder`)
///     — S04 replaces it with the real kind:9 timeline.
///
/// Three distinct data states (T05):
///   - `isSearching && tree.roots.isEmpty` → `LoadingView`
///   - `kernelIsDead` → `ErrorStateView`
///   - otherwise empty → `EmptyStateView`
///   - otherwise → the chat-style forest.
struct GroupTreeView: View {
    @EnvironmentObject private var model: KernelModel
    @State private var expandedGroups = Set<String>()

    var body: some View {
        let tree = model.groupTree

        Group {
            if model.kernelIsDead {
                ErrorStateView(
                    message: "The background service stopped. Relaunch the app to recover."
                )
            } else if model.discoveredGroups.isSearching && tree.roots.isEmpty {
                LoadingView(label: "Discovering groups on \(model.discoveredGroups.hostRelayUrl)…")
            } else if tree.roots.isEmpty {
                EmptyStateView(
                    title: "No Groups",
                    message: "Discovery has not returned any groups yet."
                )
            } else {
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 0) {
                        ForEach(tree.roots) { node in
                            GroupTreeRow(
                                node: node,
                                tree: tree,
                                depth: 0,
                                expandedGroups: $expandedGroups
                            )
                        }
                    }
                    .padding(.top, 8)
                }
                .background(Color(.systemBackground))
            }
        }
        .navigationTitle(navigationTitle(tree: tree))
        .navigationBarTitleDisplayMode(.large)
        .navigationDestination(for: String.self) { groupId in
            GroupTimelineView(groupId: groupId)
        }
        .task {
            if model.discoveredGroups.hostRelayUrl.isEmpty {
                model.openGroupDiscovery(hostRelayUrl: "wss://nip29.f7z.io")
            }
        }
        .onChange(of: tree.totalCount) { _, count in
            gtLog.info("tree updated: total=\(count, privacy: .public) roots=\(tree.roots.count, privacy: .public) searching=\(model.discoveredGroups.isSearching, privacy: .public)")
        }
    }

    private func navigationTitle(tree: GroupTreeSnapshot) -> String {
        if tree.totalCount == 0 {
            return "29er"
        }
        let suffix = tree.totalCount == 1 ? "1 group" : "\(tree.totalCount) groups"
        return "29er · \(suffix)"
    }
}

/// Recursive row view. Branches render an explicit expand control next to the
/// branch's own `NavigationLink` (D003 — branch nodes are real groups with
/// their own timeline, not just expand/collapse folders). Leaves render a
/// plain `NavigationLink`.
struct GroupTreeRow: View {
    let node: GroupTreeNode
    let tree: GroupTreeSnapshot
    let depth: Int
    @Binding var expandedGroups: Set<String>

    private var isExpanded: Bool { expandedGroups.contains(node.groupId) }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 6) {
                if node.isBranch {
                    Button(action: toggle) {
                        Image(systemName: isExpanded ? "chevron.down" : "chevron.right")
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(.secondary)
                            .frame(width: 24, height: 44)
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                } else {
                    Color.clear.frame(width: 24, height: 44)
                }

                NavigationLink(value: node.groupId) {
                    GroupRowLabel(node: node)
                }
                .buttonStyle(.plain)
            }
            .padding(.leading, CGFloat(12 + depth * 18))
            .padding(.trailing, 12)
            .padding(.vertical, 8)

            Divider()
                .padding(.leading, CGFloat(82 + depth * 18))

            if node.isBranch && isExpanded {
                ForEach(children) { child in
                    GroupTreeRow(
                        node: child,
                        tree: tree,
                        depth: depth + 1,
                        expandedGroups: $expandedGroups
                    )
                }
            }
        }
    }

    private var children: [GroupTreeNode] {
        node.childIds.compactMap { tree.allNodes[$0] }
    }

    private func toggle() {
        if isExpanded {
            expandedGroups.remove(node.groupId)
        } else {
            expandedGroups.insert(node.groupId)
        }
    }
}

/// Label content shared by branch and leaf rows. Swift renders the
/// Rust-owned list read model: group name, latest direct kind:9 preview, and
/// aggregate unread count for the group plus descendants.
struct GroupRowLabel: View {
    let node: GroupTreeNode

    var body: some View {
        HStack(spacing: 12) {
            ZStack {
                Circle()
                    .fill(Color.blue)
                Text(initials)
                    .font(.system(size: 17, weight: .semibold))
                    .foregroundStyle(.white)
            }
            .frame(width: 52, height: 52)

            VStack(alignment: .leading, spacing: 4) {
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    Text(node.displayName)
                        .font(.system(size: 17, weight: .semibold))
                        .foregroundStyle(.primary)
                        .lineLimit(1)

                    Spacer(minLength: 8)

                    if node.unreadCount > 0 {
                        Text(unreadText)
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(.white)
                            .monospacedDigit()
                            .padding(.horizontal, node.unreadCount > 9 ? 7 : 6)
                            .frame(minWidth: 22, minHeight: 22)
                            .background(Capsule().fill(Color.accentColor))
                    }
                }

                HStack(spacing: 6) {
                    Text(previewText)
                        .font(.subheadline)
                        .foregroundStyle(node.hasLastMessage ? .secondary : .tertiary)
                        .lineLimit(1)

                    Spacer(minLength: 8)
                }
            }
        }
        .contentShape(Rectangle())
    }

    private var initials: String {
        let pieces = node.displayName
            .split(separator: " ")
            .prefix(2)
            .compactMap { $0.first }
        let value = String(pieces).uppercased()
        return value.isEmpty ? "#" : value
    }

    private var previewText: String {
        guard let preview = node.lastMessagePreview?.trimmingCharacters(in: .whitespacesAndNewlines),
              !preview.isEmpty
        else {
            return "No messages yet"
        }
        return preview
    }

    private var unreadText: String {
        node.unreadCount > 99 ? "99+" : "\(node.unreadCount)"
    }
}

/// Projection-backed destination pushed by `NavigationLink(value:)`.
struct GroupTimelineView: View {
    @EnvironmentObject private var model: KernelModel
    let groupId: String

    @State private var draft = ""
    @State private var pendingMessages: [PendingGroupMessage] = []
    @FocusState private var composerFocused: Bool

    private var node: GroupTreeNode? {
        model.groupTree.allNodes[groupId]
    }

    private var title: String {
        node?.displayName ?? groupId
    }

    private var visibleMessages: [GroupChatMessage] {
        // Projection owns the newest-first data contract. Chat presentation
        // reads chronologically so the newest item anchors above the composer.
        Array(model.groupChat.messages.reversed())
    }

    private var trimmedDraft: String {
        draft.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private var canSend: Bool {
        !trimmedDraft.isEmpty && node != nil && !model.kernelIsDead
    }

    private var mentionSuggestions: [String] {
        let token = currentMentionToken(in: draft)
        let authors = model.groupChat.messages
            .map(\.pubkey)
            .reduce(into: [String]()) { result, pubkey in
                if !result.contains(pubkey) {
                    result.append(pubkey)
                }
            }
        guard let token else { return [] }
        let needle = token.lowercased()
        return authors
            .filter { needle.isEmpty || $0.shortHex.lowercased().contains(needle) || $0.lowercased().contains(needle) }
            .prefix(4)
            .map { $0 }
    }

    var body: some View {
        ScrollViewReader { proxy in
            VStack(spacing: 0) {
                if model.kernelIsDead {
                    ErrorStateView(
                        message: "The background service stopped. Relaunch the app to recover."
                    )
                } else if visibleMessages.isEmpty && pendingMessages.isEmpty {
                    emptyChat
                } else {
                    messageStream(proxy: proxy)
                }

                composer
            }
            .background(Color(.systemBackground))
            .navigationTitle(title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .principal) {
                    VStack(spacing: 1) {
                        HStack(spacing: 5) {
                            Text(title)
                                .font(.headline)
                                .lineLimit(1)

                            if let node, !node.isOpen {
                                Image(systemName: "lock.fill")
                                    .font(.caption2)
                                    .foregroundStyle(.secondary)
                            }
                        }

                        if let node {
                            Text(roomChromeSubtitle(node))
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                                .lineLimit(1)
                        }
                    }
                }

                ToolbarItem(placement: .navigationBarTrailing) {
                    if let node {
                        Label(
                            node.memberCount == 1 ? "1 member" : "\(node.memberCount) members",
                            systemImage: "person.2"
                        )
                        .labelStyle(.iconOnly)
                        .foregroundStyle(.secondary)
                        .accessibilityLabel(node.memberCount == 1 ? "1 member" : "\(node.memberCount) members")
                    }
                }
            }
            .task(id: groupId) {
                model.openGroupTimeline(groupId)
            }
            .onChange(of: model.groupChat.messages) { _, _ in
                reconcilePending()
                scrollToBottom(proxy)
            }
            .onChange(of: pendingMessages.count) { _, _ in
                scrollToBottom(proxy)
            }
        }
    }

    private var emptyChat: some View {
        ContentUnavailableView(
            "No messages yet",
            systemImage: "bubble.left.and.bubble.right",
            description: Text("Start the conversation in this NIP-29 group.")
        )
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private func messageStream(proxy: ScrollViewProxy) -> some View {
        ScrollView {
            LazyVStack(spacing: 10) {
                ForEach(visibleMessages) { message in
                    GroupMessageRow(
                        message: message,
                        isOwnMessage: message.pubkey == model.activeAccountPubkey,
                        onReact: {
                            model.reactToGroupMessage(
                                groupId: groupId,
                                eventId: message.id,
                                eventAuthorPubkey: message.pubkey
                            )
                        }
                    )
                    .id(message.id)
                }

                ForEach(pendingMessages) { message in
                    PendingMessageRow(message: message)
                        .id(message.id)
                }

                Color.clear
                    .frame(height: 1)
                    .id("chat-bottom")
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 12)
        }
        .onAppear { scrollToBottom(proxy, animated: false) }
    }

    private var composer: some View {
        VStack(spacing: 0) {
            if !mentionSuggestions.isEmpty {
                mentionSuggestionBar
            }

            HStack(alignment: .bottom, spacing: 8) {
                TextField("Message \(title)", text: $draft, axis: .vertical)
                    .focused($composerFocused)
                    .font(.body)
                    .textFieldStyle(.plain)
                    .lineLimit(1...4)
                    .padding(.horizontal, 11)
                    .padding(.vertical, 8)
                    .background(
                        RoundedRectangle(cornerRadius: 8, style: .continuous)
                            .fill(Color(.secondarySystemBackground))
                    )
                    .overlay(
                        RoundedRectangle(cornerRadius: 8, style: .continuous)
                            .stroke(Color(.separator), lineWidth: 0.5)
                    )
                    .accessibilityIdentifier("group-chat-message-editor")

                Button(action: sendDraft) {
                    Image(systemName: "arrow.up.circle.fill")
                        .font(.system(size: 29, weight: .semibold))
                        .symbolRenderingMode(.hierarchical)
                        .foregroundStyle(canSend ? Color.accentColor : Color.secondary)
                        .frame(width: 32, height: 32)
                }
                .buttonStyle(.plain)
                .disabled(!canSend)
                .accessibilityLabel("Send message")
                .accessibilityIdentifier("group-chat-send-button")
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 7)
        }
        .background(.regularMaterial)
        .overlay(alignment: .top) { Divider() }
    }

    private var mentionSuggestionBar: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(mentionSuggestions, id: \.self) { pubkey in
                    Button {
                        acceptMention(pubkey)
                    } label: {
                        Label(pubkey.shortHex, systemImage: "at")
                            .font(.caption.weight(.semibold))
                            .padding(.horizontal, 10)
                            .padding(.vertical, 6)
                            .background(
                                Capsule().fill(Color(.tertiarySystemBackground))
                            )
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("Mention \(pubkey.shortHex)")
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
        }
        .background(Color(.secondarySystemBackground))
        .overlay(alignment: .bottom) { Divider() }
    }

    private func sendDraft() {
        let text = trimmedDraft
        guard canSend else { return }

        let pending = PendingGroupMessage(content: text)
        pendingMessages.append(pending)
        let accepted = model.sendGroupMessage(groupId: groupId, content: text)
        if !accepted, let index = pendingMessages.firstIndex(where: { $0.id == pending.id }) {
            pendingMessages[index].state = .failed
        }

        UIImpactFeedbackGenerator(style: .light).impactOccurred()
        draft = ""
        composerFocused = true

        Task { @MainActor in
            try? await Task.sleep(for: .seconds(12))
            if let index = pendingMessages.firstIndex(where: { $0.id == pending.id }),
               pendingMessages[index].state == .sending
            {
                pendingMessages[index].state = .queued
            }
        }
    }

    private func roomChromeSubtitle(_ node: GroupTreeNode) -> String {
        var pieces: [String] = []
        if node.memberCount > 0 {
            pieces.append(node.memberCount == 1 ? "1 member" : "\(node.memberCount) members")
        }
        pieces.append(node.isOpen ? "open" : "closed")
        return pieces.joined(separator: " • ")
    }

    private func reconcilePending() {
        guard let activePubkey = model.activeAccountPubkey else { return }
        let delivered = model.groupChat.messages.filter { $0.pubkey == activePubkey }
        pendingMessages.removeAll { pending in
            delivered.contains { delivered in
                delivered.content == pending.content && delivered.createdAt >= pending.createdAtSeconds - 10
            }
        }
    }

    private func scrollToBottom(_ proxy: ScrollViewProxy, animated: Bool = true) {
        let action = {
            proxy.scrollTo("chat-bottom", anchor: .bottom)
        }
        if animated {
            withAnimation(.easeOut(duration: 0.22), action)
        } else {
            action()
        }
    }

    private func currentMentionToken(in text: String) -> String? {
        guard let last = text.split(separator: " ", omittingEmptySubsequences: false).last,
              last.hasPrefix("@")
        else { return nil }
        return String(last.dropFirst())
    }

    private func acceptMention(_ pubkey: String) {
        let mention = "@\(pubkey.shortHex)"
        var parts = draft.split(separator: " ", omittingEmptySubsequences: false).map(String.init)
        if parts.isEmpty {
            draft = mention + " "
        } else {
            parts[parts.count - 1] = mention
            draft = parts.joined(separator: " ") + " "
        }
        composerFocused = true
    }
}

private struct GroupMessageRow: View {
    let message: GroupChatMessage
    let isOwnMessage: Bool
    let onReact: () -> Void

    var body: some View {
        HStack(alignment: .bottom, spacing: 8) {
            if isOwnMessage {
                Spacer(minLength: 42)
            } else {
                ChatAvatar(seed: message.pubkey)
            }

            VStack(alignment: isOwnMessage ? .trailing : .leading, spacing: 4) {
                HStack(spacing: 6) {
                    if !isOwnMessage {
                        Text(message.pubkey.shortHex)
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }

                    Text(message.createdAt.relativeTimeFromUnixSeconds)
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                }

                Text(message.content)
                    .font(.body)
                    .foregroundStyle(isOwnMessage ? .white : .primary)
                    .textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 9)
                    .background(
                        RoundedRectangle(cornerRadius: 8, style: .continuous)
                            .fill(isOwnMessage ? Color.accentColor : Color(.secondarySystemBackground))
                    )
            }

            if !isOwnMessage {
                Spacer(minLength: 42)
            }
        }
        .contextMenu {
            Button(action: onReact) {
                Label("React", systemImage: "heart")
            }
            Button {
                UIPasteboard.general.string = message.content
            } label: {
                Label("Copy Text", systemImage: "doc.on.doc")
            }
            Button {
                UIPasteboard.general.string = message.id
            } label: {
                Label("Copy Event ID", systemImage: "number")
            }
        }
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("group-chat-message-\(message.id)")
    }
}

private struct PendingMessageRow: View {
    let message: PendingGroupMessage

    var body: some View {
        HStack(alignment: .bottom, spacing: 8) {
            Spacer(minLength: 42)

            VStack(alignment: .trailing, spacing: 4) {
                Text(message.state.label)
                    .font(.caption2)
                    .foregroundStyle(.tertiary)

                Text(message.content)
                    .font(.body)
                    .foregroundStyle(.white)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 9)
                    .background(
                        RoundedRectangle(cornerRadius: 8, style: .continuous)
                            .fill(Color.accentColor.opacity(0.72))
                    )
            }
        }
        .accessibilityIdentifier("group-chat-pending-message-\(message.id.uuidString)")
    }
}

private struct ChatAvatar: View {
    let seed: String

    var body: some View {
        ZStack {
            Circle()
                .fill(seed.pubkeyColor)
            Text(seed.displayInitials)
                .font(.caption.weight(.bold))
                .foregroundStyle(.white)
        }
        .frame(width: 32, height: 32)
        .accessibilityHidden(true)
    }
}

private struct PendingGroupMessage: Identifiable, Equatable {
    let id = UUID()
    let content: String
    let createdAt = Date()
    var state: PendingState = .sending

    var createdAtSeconds: UInt64 {
        UInt64(createdAt.timeIntervalSince1970)
    }
}

private enum PendingState: Equatable {
    case sending
    case queued
    case failed

    var label: String {
        switch self {
        case .sending:
            return "Sending"
        case .queued:
            return "Queued"
        case .failed:
            return "Not sent"
        }
    }
}

private extension String {
    var shortHex: String {
        guard count > 16 else { return self }
        return "\(prefix(8))…\(suffix(8))"
    }

    var displayInitials: String {
        let words = split(separator: " ").prefix(2)
        if words.count >= 2 {
            return words.compactMap(\.first).map { String($0).uppercased() }.joined()
        }
        return count >= 2 ? String(prefix(2)).uppercased() : ".."
    }

    var pubkeyColor: Color {
        var hash: UInt32 = 5381
        for byte in utf8 {
            hash = hash &* 33 &+ UInt32(byte)
        }
        return Color(
            hue: Double(hash % 360) / 360.0,
            saturation: 0.58,
            brightness: 0.72
        )
    }
}

private func relativeFormatter() -> RelativeDateTimeFormatter {
    let key = "TwentyNinerRelativeDateTimeFormatter"
    if let existing = Thread.current.threadDictionary[key] as? RelativeDateTimeFormatter {
        return existing
    }
    let formatter = RelativeDateTimeFormatter()
    formatter.unitsStyle = .abbreviated
    Thread.current.threadDictionary[key] = formatter
    return formatter
}

private extension UInt64 {
    var relativeTimeFromUnixSeconds: String {
        let date = Date(timeIntervalSince1970: TimeInterval(self))
        let now = Date()
        if date >= now.addingTimeInterval(-5) {
            return "now"
        }
        return relativeFormatter().localizedString(for: date, relativeTo: now)
    }
}
