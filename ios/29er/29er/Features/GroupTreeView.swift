import SwiftUI
import UIKit
import os.log

private let gtLog = Logger(subsystem: "io.f7z.app29er.bridge", category: "GroupTreeView")

/// S03 main group-tree navigation screen. Renders the Rust-owned
/// `GroupTreeSnapshot` as a push-navigation group list per D009:
///
///   - `NavigationStack` + push navigation (no `.sidebar` split column —
///     iPhone-only).
///   - Branch rows keep the branch's own `NavigationLink` (D003 — branch nodes
///     are real groups; tapping the row navigates to the group's timeline,
///     tapping the trailing chevron pushes the child groups).
///   - Leaf rows are plain chat-list rows.
///   - Pushed destination is a placeholder timeline view (`TimelinePlaceholder`)
///     — S04 replaces it with the real kind:9 timeline.
///
/// Three distinct data states (T05):
///   - `isSearching && tree.roots.isEmpty` → `LoadingView`
///   - `kernelIsDead` → `ErrorStateView`
///   - otherwise empty → `EmptyStateView`
///   - otherwise → the chat-style list.
struct GroupTreeView: View {
    @EnvironmentObject private var model: KernelModel

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
                    GroupTreeList(nodes: tree.roots, tree: tree)
                        .padding(.horizontal, 12)
                        .padding(.vertical, 10)
                }
                .background(Color(.systemGroupedBackground))
            }
        }
        .navigationTitle(navigationTitle(tree: tree))
        .navigationBarTitleDisplayMode(.large)
        .navigationDestination(for: String.self) { groupId in
            GroupTimelineView(groupId: groupId)
        }
        .navigationDestination(for: GroupChildrenRoute.self) { route in
            GroupChildrenView(parentGroupId: route.groupId)
        }
        .task {
            if model.discoveredGroups.hostRelayUrl.isEmpty {
                model.openGroupDiscovery(hostRelayUrl: defaultNip29RelayUrl)
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

private struct GroupChildrenRoute: Hashable {
    let groupId: String
}

private struct GroupTreeList: View {
    let nodes: [GroupTreeNode]
    let tree: GroupTreeSnapshot

    var body: some View {
        GlassEffectContainer(spacing: 10) {
            LazyVStack(alignment: .leading, spacing: 10) {
                ForEach(nodes) { node in
                    GroupTreeRow(node: node, tree: tree)
                }
            }
        }
    }
}

/// Branch rows expose two targets: the row opens the group's own timeline,
/// while the trailing chevron pushes a list of that group's children.
struct GroupTreeRow: View {
    let node: GroupTreeNode
    let tree: GroupTreeSnapshot

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 8) {
                NavigationLink(value: node.groupId) {
                    GroupRowLabel(node: node)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .buttonStyle(.plain)

                if !children.isEmpty {
                    NavigationLink(value: GroupChildrenRoute(groupId: node.groupId)) {
                        Image(systemName: "chevron.right")
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(.tertiary)
                            .frame(width: 44, height: 44)
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("Show child groups for \(node.displayName)")
                }
            }
            .padding(.leading, 12)
            .padding(.trailing, 12)
            .padding(.vertical, 8)
            .glassPanel(cornerRadius: 18, interactive: true)
        }
    }

    private var children: [GroupTreeNode] {
        node.childIds.compactMap { tree.allNodes[$0] }
    }
}

private struct GroupChildrenView: View {
    @EnvironmentObject private var model: KernelModel
    let parentGroupId: String

    private var parent: GroupTreeNode? {
        model.groupTree.allNodes[parentGroupId]
    }

    private var children: [GroupTreeNode] {
        guard let parent else { return [] }
        return parent.childIds.compactMap { model.groupTree.allNodes[$0] }
    }

    var body: some View {
        Group {
            if children.isEmpty {
                ContentUnavailableView(
                    "No Child Groups",
                    systemImage: "folder",
                    description: Text("This group does not have any child groups.")
                )
            } else {
                ScrollView {
                    GroupTreeList(nodes: children, tree: model.groupTree)
                        .padding(.horizontal, 12)
                        .padding(.vertical, 10)
                }
                .background(Color(.systemGroupedBackground))
            }
        }
        .navigationTitle(parent?.displayName ?? "Groups")
        .navigationBarTitleDisplayMode(.inline)
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
    @State private var selectedMentionPubkeys: Set<String> = []
    @State private var showingMembers = false
    @State private var showingJoinSheet = false
    @State private var showingLeaveSheet = false
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

    private var outboxMessages: [PublishOutboxItem] {
        let deliveredIds = Set(model.groupChat.messages.map(\.id))
        let filtered = model.publishOutbox.filter { item in
            item.kind == 9 &&
                !deliveredIds.contains(item.eventId) &&
                item.tags.contains { tag in
                    tag.count >= 2 && tag[0] == "h" && tag[1] == groupId
                }
        }
        return Array(filtered.reversed())
    }

    private var trimmedDraft: String {
        draft.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private var canSend: Bool {
        !trimmedDraft.isEmpty && node != nil && !model.kernelIsDead
    }

    private var currentMembers: [GroupMember] {
        guard model.groupMembers.groupId == groupId else { return [] }
        return model.groupMembers.members
    }

    private var membersProjectionMatchesGroup: Bool {
        model.groupMembers.groupId == groupId
    }

    private var activeMember: GroupMember? {
        guard let activePubkey = model.activeAccountPubkey else { return nil }
        return currentMembers.first { $0.pubkey == activePubkey }
    }

    private var isCurrentMember: Bool {
        activeMember != nil
    }

    private var isCurrentAdmin: Bool {
        activeMember?.admin == true
    }

    private var membershipOutboxItems: [PublishOutboxItem] {
        model.publishOutbox.filter { item in
            (item.kind == 9021 || item.kind == 9022) &&
                item.tags.contains { tag in
                    tag.count >= 2 && tag[0] == "h" && tag[1] == groupId
                }
        }
    }

    private var latestMembershipOutboxItem: PublishOutboxItem? {
        membershipOutboxItems.sorted { $0.createdAt > $1.createdAt }.first
    }

    private var membershipStatusLabel: String {
        if let item = latestMembershipOutboxItem {
            switch item.kind {
            case 9021:
                return item.status == "failed" ? "Join failed" : "Joining"
            case 9022:
                return item.status == "failed" ? "Leave failed" : "Leaving"
            default:
                break
            }
        }
        if !membersProjectionMatchesGroup {
            return "Checking"
        }
        if isCurrentAdmin {
            return "Admin"
        }
        if isCurrentMember {
            return "Member"
        }
        if node?.isOpen == true {
            return "Not joined"
        }
        return "Invite required"
    }

    private var membershipStatusIcon: String {
        if let item = latestMembershipOutboxItem {
            return item.kind == 9022 ? "person.badge.minus" : "person.badge.plus"
        }
        if !membersProjectionMatchesGroup {
            return "clock"
        }
        if isCurrentAdmin {
            return "shield.fill"
        }
        if isCurrentMember {
            return "checkmark.circle.fill"
        }
        return node?.isOpen == true ? "person.crop.circle.badge.plus" : "lock.fill"
    }

    private var hasPendingMembershipAction: Bool {
        latestMembershipOutboxItem != nil
    }

    private var mentionSuggestions: [GroupMember] {
        let token = currentMentionToken(in: draft)
        guard let token else { return [] }
        let needle = token.lowercased()
        return currentMembers
            .filter { member in
                needle.isEmpty ||
                    member.title.lowercased().contains(needle) ||
                    member.pubkey.shortHex.lowercased().contains(needle) ||
                    member.pubkey.lowercased().contains(needle)
            }
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
                } else if visibleMessages.isEmpty && outboxMessages.isEmpty {
                    emptyChat
                } else {
                    messageStream(proxy: proxy)
                }

                membershipBar
                composer
            }
            .background(Color(.systemGroupedBackground))
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
                        Button {
                            showingMembers = true
                        } label: {
                            Label(
                                node.memberCount == 1 ? "1 member" : "\(node.memberCount) members",
                                systemImage: "person.2"
                            )
                        }
                        .labelStyle(.iconOnly)
                        .accessibilityLabel(node.memberCount == 1 ? "1 member" : "\(node.memberCount) members")
                    }
                }
            }
            .sheet(isPresented: $showingMembers) {
                MemberListSheet(title: title, members: currentMembers)
                    .presentationDetents([.medium, .large])
            }
            .sheet(isPresented: $showingJoinSheet) {
                JoinGroupSheet(
                    title: title,
                    requiresInviteCode: node?.isOpen == false,
                    onJoin: { inviteCode, reason in
                        model.joinGroup(
                            groupId: groupId,
                            inviteCode: inviteCode,
                            reason: reason
                        )
                    }
                )
                .presentationDetents([.medium])
            }
            .sheet(isPresented: $showingLeaveSheet) {
                LeaveGroupSheet(
                    title: title,
                    onLeave: { reason in
                        model.leaveGroup(groupId: groupId, reason: reason)
                    }
                )
                .presentationDetents([.medium])
            }
            .task(id: groupId) {
                model.openGroupTimeline(groupId)
            }
            .onChange(of: model.groupChat.messages) { _, _ in
                scrollToBottom(proxy)
            }
            .onChange(of: outboxMessages.count) { _, _ in
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
        .padding(24)
        .frame(maxWidth: 360)
        .glassPanel(cornerRadius: 22)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var membershipBar: some View {
        HStack(spacing: 10) {
            Label(membershipStatusLabel, systemImage: membershipStatusIcon)
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(.secondary)
                .lineLimit(1)

            if let item = latestMembershipOutboxItem {
                if item.canRetry {
                    Button {
                        model.retryPublish(item)
                    } label: {
                        Image(systemName: "arrow.clockwise.circle.fill")
                            .font(.subheadline.weight(.semibold))
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("Retry membership action")
                }

                Text(item.status.pendingDisplayLabel)
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.tertiary)
            }

            Spacer(minLength: 8)

            if isCurrentMember {
                Button {
                    showingLeaveSheet = true
                } label: {
                    Label("Leave", systemImage: "person.badge.minus")
                }
                .buttonStyle(.glass)
                .disabled(hasPendingMembershipAction)
            } else {
                Button {
                    showingJoinSheet = true
                } label: {
                    Label("Join", systemImage: "person.badge.plus")
                }
                .buttonStyle(.glassProminent)
                .disabled(
                    hasPendingMembershipAction ||
                        !membersProjectionMatchesGroup ||
                        node == nil ||
                        model.kernelIsDead
                )
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .background(.thinMaterial)
        .overlay(alignment: .top) { Divider() }
    }

    private func messageStream(proxy: ScrollViewProxy) -> some View {
        ScrollView {
            GlassEffectContainer(spacing: 12) {
                LazyVStack(spacing: 10) {
                    ForEach(visibleMessages) { message in
                        let pending = outboxItem(for: message.id)
                        GroupMessageRow(
                            message: message,
                            isOwnMessage: message.pubkey == model.activeAccountPubkey,
                            pendingStatus: pending?.status,
                            canRetry: pending?.canRetry ?? false,
                            onRetry: {
                                if let pending {
                                    model.retryPublish(pending)
                                }
                            },
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

                    ForEach(outboxMessages) { message in
                        PendingMessageRow(
                            message: message,
                            onRetry: { model.retryPublish(message) }
                        )
                        .id(message.id)
                    }

                    Color.clear
                        .frame(height: 1)
                        .id("chat-bottom")
                }
                .padding(.horizontal, 14)
                .padding(.vertical, 12)
            }
        }
        .onAppear { scrollToBottom(proxy, animated: false) }
    }

    private var composer: some View {
        GlassEffectContainer(spacing: 10) {
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
                        .padding(.horizontal, 12)
                        .padding(.vertical, 10)
                        .glassPanel(cornerRadius: 16, interactive: true)
                        .accessibilityIdentifier("group-chat-message-editor")

                    Button(action: sendDraft) {
                        Image(systemName: "arrow.up")
                            .font(.system(size: 18, weight: .bold))
                            .foregroundStyle(canSend ? Color.accentColor : Color.secondary)
                            .frame(width: 36, height: 36)
                            .glassPanel(cornerRadius: 18, interactive: canSend)
                    }
                    .buttonStyle(.plain)
                    .disabled(!canSend)
                    .accessibilityLabel("Send message")
                    .accessibilityIdentifier("group-chat-send-button")
                }
                .padding(.horizontal, 10)
                .padding(.vertical, 8)
            }
        }
        .background(.ultraThinMaterial)
        .overlay(alignment: .top) { Divider() }
    }

    private var mentionSuggestionBar: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(mentionSuggestions) { member in
                    Button {
                        acceptMention(member)
                    } label: {
                        Label(member.title, systemImage: "at")
                            .font(.caption.weight(.semibold))
                            .padding(.horizontal, 10)
                            .padding(.vertical, 6)
                            .glassEffect(.regular.interactive(), in: .capsule)
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("Mention \(member.title)")
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
        }
        .background(.thinMaterial)
        .overlay(alignment: .bottom) { Divider() }
    }

    private func sendDraft() {
        let text = trimmedDraft
        guard canSend else { return }

        let accepted = model.sendGroupMessage(
            groupId: groupId,
            content: text,
            mentionPubkeys: activeMentionPubkeys(in: text)
        )
        guard accepted else {
            composerFocused = true
            return
        }

        UIImpactFeedbackGenerator(style: .light).impactOccurred()
        draft = ""
        selectedMentionPubkeys.removeAll()
        composerFocused = true
    }

    private func roomChromeSubtitle(_ node: GroupTreeNode) -> String {
        var pieces: [String] = []
        if node.memberCount > 0 {
            pieces.append(node.memberCount == 1 ? "1 member" : "\(node.memberCount) members")
        }
        pieces.append(node.isOpen ? "open" : "closed")
        return pieces.joined(separator: " • ")
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

    private func acceptMention(_ member: GroupMember) {
        selectedMentionPubkeys.insert(member.pubkey)
        let mention = "@\(member.title)"
        var parts = draft.split(separator: " ", omittingEmptySubsequences: false).map(String.init)
        if parts.isEmpty {
            draft = mention + " "
        } else {
            parts[parts.count - 1] = mention
            draft = parts.joined(separator: " ") + " "
        }
        composerFocused = true
    }

    private func activeMentionPubkeys(in text: String) -> [String] {
        let selectedMembers = currentMembers
            .filter { member in
                selectedMentionPubkeys.contains(member.pubkey) &&
                    (text.contains("@\(member.title)") || text.contains("@\(member.pubkey.shortHex)"))
            }
            .map(\.pubkey)
        return Array(Set(selectedMembers + rawMentionIdentifiers(in: text))).sorted()
    }

    private func rawMentionIdentifiers(in text: String) -> [String] {
        text.split(whereSeparator: \.isWhitespace)
            .compactMap { raw -> String? in
                guard raw.hasPrefix("@") else { return nil }
                let token = raw
                    .dropFirst()
                    .trimmingCharacters(in: CharacterSet(charactersIn: ".,:;!?)]}"))
                guard looksLikeRawMentionIdentifier(token) else { return nil }
                return String(token)
            }
    }

    private func looksLikeRawMentionIdentifier(_ token: String) -> Bool {
        token.hasPrefix("npub1") ||
            (token.count == 64 && token.allSatisfy(\.isHexDigit))
    }

    private func outboxItem(for eventId: String) -> PublishOutboxItem? {
        model.publishOutbox.first { $0.eventId == eventId }
    }
}

private struct JoinGroupSheet: View {
    let title: String
    let requiresInviteCode: Bool
    let onJoin: (String?, String?) -> Bool

    @Environment(\.dismiss) private var dismiss
    @State private var inviteCode = ""
    @State private var reason = ""
    @State private var error: String?

    private var trimmedInviteCode: String {
        inviteCode.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private var trimmedReason: String {
        reason.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField("Invite code", text: $inviteCode)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                    TextField("Reason", text: $reason, axis: .vertical)
                        .lineLimit(2...4)
                }

                if let error {
                    Section {
                        Text(error)
                            .foregroundStyle(.red)
                    }
                }
            }
            .navigationTitle(title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Join") {
                        let accepted = onJoin(
                            trimmedInviteCode.isEmpty ? nil : trimmedInviteCode,
                            trimmedReason.isEmpty ? nil : trimmedReason
                        )
                        if accepted {
                            dismiss()
                        } else {
                            error = "Could not send join request."
                        }
                    }
                    .disabled(requiresInviteCode && trimmedInviteCode.isEmpty)
                }
            }
        }
    }
}

private struct LeaveGroupSheet: View {
    let title: String
    let onLeave: (String?) -> Bool

    @Environment(\.dismiss) private var dismiss
    @State private var reason = ""
    @State private var error: String?

    private var trimmedReason: String {
        reason.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField("Reason", text: $reason, axis: .vertical)
                        .lineLimit(2...4)
                }

                if let error {
                    Section {
                        Text(error)
                            .foregroundStyle(.red)
                    }
                }
            }
            .navigationTitle(title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button(role: .destructive) {
                        let accepted = onLeave(trimmedReason.isEmpty ? nil : trimmedReason)
                        if accepted {
                            dismiss()
                        } else {
                            error = "Could not send leave request."
                        }
                    } label: {
                        Text("Leave")
                    }
                }
            }
        }
    }
}

private struct MemberListSheet: View {
    let title: String
    let members: [GroupMember]

    var body: some View {
        NavigationStack {
            Group {
                if members.isEmpty {
                    ContentUnavailableView(
                        "No members",
                        systemImage: "person.2.slash",
                        description: Text("This group has not published a member list yet.")
                    )
                    .padding(24)
                    .frame(maxWidth: 360)
                    .glassPanel(cornerRadius: 22)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .background(Color(.systemGroupedBackground))
                } else {
                    List(members) { member in
                        HStack(spacing: 10) {
                            ChatAvatar(seed: member.pubkey)
                                .frame(width: 30, height: 30)

                            VStack(alignment: .leading, spacing: 2) {
                                HStack(spacing: 6) {
                                    Text(member.title)
                                        .font(.body.weight(.medium))
                                        .lineLimit(1)
                                    if member.admin {
                                        Image(systemName: "shield.fill")
                                            .font(.caption)
                                            .foregroundStyle(.secondary)
                                            .accessibilityLabel("Admin")
                                    }
                                }
                                Text(member.pubkey.shortHex)
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                        }
                        .padding(.vertical, 3)
                        .listRowBackground(Color.clear)
                    }
                    .listStyle(.insetGrouped)
                    .scrollContentBackground(.hidden)
                    .background(Color(.systemGroupedBackground))
                }
            }
            .navigationTitle(title)
            .navigationBarTitleDisplayMode(.inline)
        }
    }
}

private struct GroupMessageRow: View {
    let message: GroupChatMessage
    let isOwnMessage: Bool
    let pendingStatus: String?
    let canRetry: Bool
    let onRetry: () -> Void
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

                    if let pendingStatus {
                        if canRetry {
                            Button(action: onRetry) {
                                Image(systemName: "arrow.clockwise.circle.fill")
                                    .font(.caption.weight(.semibold))
                                    .symbolRenderingMode(.hierarchical)
                            }
                            .buttonStyle(.plain)
                            .accessibilityLabel("Retry message")
                        }

                        Text(pendingStatus.pendingDisplayLabel)
                            .font(.caption2.weight(.medium))
                            .foregroundStyle(.tertiary)
                    }
                }

                Text(message.content)
                    .font(.body)
                    .foregroundStyle(isOwnMessage ? .white : .primary)
                    .textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 9)
                    .glassEffect(
                        isOwnMessage ? .regular.tint(Color.accentColor).interactive() : .regular.interactive(),
                        in: .rect(cornerRadius: 16)
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
    let message: PublishOutboxItem
    let onRetry: () -> Void

    var body: some View {
        HStack(alignment: .bottom, spacing: 8) {
            Spacer(minLength: 42)

            VStack(alignment: .trailing, spacing: 4) {
                HStack(spacing: 6) {
                    if message.canRetry {
                        Button(action: onRetry) {
                            Image(systemName: "arrow.clockwise.circle.fill")
                                .font(.caption.weight(.semibold))
                                .symbolRenderingMode(.hierarchical)
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel("Retry message")
                    }

                    Text(message.status.pendingDisplayLabel)
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                }

                Text(message.content)
                    .font(.body)
                    .foregroundStyle(.white)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 9)
                    .glassEffect(.regular.tint(Color.accentColor.opacity(0.72)).interactive(), in: .rect(cornerRadius: 16))
            }
        }
        .accessibilityIdentifier("group-chat-pending-message-\(message.id)")
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

private extension String {
    var pendingDisplayLabel: String {
        switch self {
        case "sending":
            return "Sending"
        case "retrying":
            return "Retrying"
        case "pending", "queued":
            return "Queued"
        case "failed":
            return "Not sent"
        default:
            return isEmpty ? "Queued" : self
        }
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
