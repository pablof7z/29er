import SwiftUI
import UIKit
import os.log

private let gtLog = Logger(subsystem: "io.f7z.app29er.bridge", category: "GroupTreeView")

/// S03 main group-tree navigation screen. Renders the Rust-owned
/// `GroupTreeSnapshot` as a push-navigation group list per D009:
///
///   - `NavigationStack` + push navigation (no `.sidebar` split column —
///     iPhone-only).
///   - Rows are single-target chat-list rows. Branch nodes are real groups, so
///     tapping any row opens that group's timeline.
///   - Child groups are reached from the room toolbar once the parent room is
///     open, keeping the root list free of split row targets.
///
/// Three distinct data states (T05):
///   - `isSearching && tree.roots.isEmpty` → `LoadingView`
///   - `kernelIsDead` → `ErrorStateView`
///   - otherwise empty → `EmptyStateView`
///   - otherwise → the chat-style list.
struct GroupTreeView: View {
    @EnvironmentObject private var model: KernelModel
    @State private var showingRelaySelector = false

    var body: some View {
        let tree = model.groupTree

        Group {
            if model.kernelIsDead {
                ErrorStateView(
                    message: "The background service stopped. Relaunch the app to recover."
                )
            } else if tree.roots.isEmpty {
                EmptyStateView(
                    title: "No Rooms",
                    message: "Rooms will appear here when this relay publishes them."
                )
            } else {
                GroupTreeList(nodes: tree.roots, tree: tree)
            }
        }
        .navigationTitle("")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .principal) {
                Button {
                    showingRelaySelector = true
                } label: {
                    HStack(spacing: 4) {
                        Text(model.activeRelayTitle)
                            .font(.headline)
                            .lineLimit(1)
                        Image(systemName: "chevron.down")
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                    }
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Relay")
            }
        }
        .sheet(isPresented: $showingRelaySelector) {
            RelaySelectorSheet()
                .environmentObject(model)
        }
        .navigationDestination(for: String.self) { groupId in
            GroupEventsView(groupId: groupId)
        }
        .navigationDestination(for: GroupChildrenRoute.self) { route in
            GroupChildrenView(parentGroupId: route.groupId)
        }
        .task {
            // Host relay comes from the Rust-owned relay selector projection.
            // The selector falls back to 29er's default relay when no NIP-51
            // relay set exists yet.
            let activeRelay = model.relaySelector.activeRelayUrl
            if model.discoveredGroups.hostRelayUrl.isEmpty, !activeRelay.isEmpty {
                model.openGroupDiscovery(hostRelayUrl: activeRelay)
            }
        }
        .onChange(of: tree.totalCount) { _, count in
            gtLog.info("tree updated: total=\(count, privacy: .public) roots=\(tree.roots.count, privacy: .public) searching=\(model.discoveredGroups.isSearching, privacy: .public)")
        }
    }

}

private struct RelaySelectorSheet: View {
    @EnvironmentObject private var model: KernelModel
    @Environment(\.dismiss) private var dismiss
    @State private var newRelayUrl = ""

    private var trimmedNewRelayUrl: String {
        newRelayUrl.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    var body: some View {
        NavigationStack {
            List {
                Section {
                    ForEach(model.relaySelector.relays) { row in
                        Button {
                            if model.selectNip29Relay(row.relayUrl) {
                                dismiss()
                            }
                        } label: {
                            HStack(spacing: 12) {
                                VStack(alignment: .leading, spacing: 3) {
                                    Text(model.relayDisplayName(for: row.relayUrl))
                                        .font(.body)
                                        .foregroundStyle(.primary)
                                        .lineLimit(1)
                                    Text(row.relayUrl)
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                        .lineLimit(1)
                                }
                                Spacer()
                                if row.selected {
                                    Image(systemName: "checkmark")
                                        .font(.headline)
                                        .foregroundStyle(.tint)
                                }
                            }
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                    }
                    .onDelete(perform: removeRelays)
                }

                Section {
                    HStack(spacing: 10) {
                        TextField("wss://relay.example", text: $newRelayUrl)
                            .keyboardType(.URL)
                            .textInputAutocapitalization(.never)
                            .autocorrectionDisabled()
                        Button {
                            addRelay()
                        } label: {
                            Image(systemName: "plus.circle.fill")
                                .font(.title3)
                        }
                        .disabled(trimmedNewRelayUrl.isEmpty)
                    }
                }
            }
            .navigationTitle("Relay")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Done") {
                        dismiss()
                    }
                }
            }
        }
    }

    private func addRelay() {
        let relayUrl = trimmedNewRelayUrl
        guard !relayUrl.isEmpty else { return }
        if model.addNip29Relay(relayUrl) {
            newRelayUrl = ""
        }
    }

    private func removeRelays(at offsets: IndexSet) {
        let relays = offsets
            .compactMap { index in model.relaySelector.relays.indices.contains(index) ? model.relaySelector.relays[index] : nil }
            .filter(\.fromNip51)
        for relay in relays {
            _ = model.removeNip29Relay(relay.relayUrl)
        }
    }
}

private struct GroupChildrenRoute: Hashable {
    let groupId: String
}

private struct GroupTreeList: View {
    let nodes: [GroupTreeNode]
    let tree: GroupTreeSnapshot

    var body: some View {
        List {
            ForEach(nodes) { node in
                GroupTreeRow(node: node, tree: tree)
                    .listRowInsets(EdgeInsets(top: 0, leading: 16, bottom: 0, trailing: 12))
                    .listRowSeparator(.visible)
            }
        }
        .listStyle(.plain)
        .scrollContentBackground(.hidden)
        .background(Color(.systemBackground))
    }
}

/// A single chat-list row. Subroom browsing is exposed from the room toolbar,
/// not as a competing root-list tap target.
struct GroupTreeRow: View {
    let node: GroupTreeNode
    let tree: GroupTreeSnapshot

    var body: some View {
        NavigationLink(value: node.groupId) {
            GroupRowLabel(node: node)
        }
        .accessibilityIdentifier("group-row-\(node.groupId)")
        .frame(minHeight: 60)
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
                GroupTreeList(nodes: children, tree: model.groupTree)
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
                    .fill(node.groupId.pubkeyColor)
                Text(initials)
                    .font(.system(size: 16, weight: .semibold))
                    .foregroundStyle(.white)
            }
            .frame(width: 46, height: 46)

            VStack(alignment: .leading, spacing: 3) {
                Text(node.displayName)
                    .font(.system(size: 17, weight: .semibold))
                    .foregroundStyle(.primary)
                    .lineLimit(1)

                Text(previewText)
                    .font(.subheadline)
                    .foregroundStyle(node.hasLastMessage ? .secondary : .tertiary)
                    .lineLimit(1)
            }

            Spacer(minLength: 8)

            VStack(alignment: .trailing, spacing: 5) {
                if node.hasLastMessage {
                    Text(node.lastMessageCreatedAt.relativeTimeFromUnixSeconds)
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                        .lineLimit(1)
                }

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
        if node.typingCount == 1 {
            return "typing..."
        }
        if node.typingCount > 1 {
            return "\(node.typingCount) typing..."
        }
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
struct GroupEventsView: View {
    @EnvironmentObject private var model: KernelModel
    let groupId: String

    @State private var draft = ""
    @State private var selectedMentionPubkeys: Set<String> = []
    @State private var showingMembers = false
    @State private var showingJoinSheet = false
    @State private var showingLeaveSheet = false
    @State private var showingAdminSheet = false

    private var node: GroupTreeNode? {
        model.groupTree.allNodes[groupId]
    }

    private var title: String {
        node?.displayName ?? groupId
    }

    private var visibleMessages: [GroupChatMessage] {
        // The Rust-owned `app.29er.group_chat` projection is the single ordered,
        // deduped, enriched message list. The shell no longer joins
        // `publish_outbox` against chat, walks raw `["h", groupId]` tags,
        // tokenizes content, dedups by event id, or reorders. The projection is
        // newest-first; chat presentation reads chronologically so the newest
        // item anchors above the composer. Per-message delivery status is a thin
        // eventId-keyed decoration via `outboxItem(for:)` (the kernel owns the
        // status token + retry decision), not a tag/kind policy join.
        Array(model.groupChat.messages.reversed())
    }

    private var canCompose: Bool {
        // Fix #3: composing is gated on the projection-emitted viewer membership
        // flag, not a Swift roster scan. `isCurrentMember` reads `node.isMember`,
        // which implies the node exists.
        isCurrentMember && !model.kernelIsDead
    }

    // The selected group's member list comes from the Rust-opened
    // `nmp.nip29.group_roster` read session. Membership/admin gating does not
    // depend on this roster; it reads the JoinedGroupsProjection-backed
    // `node.isMember` / `node.isAdmin` flags from the group-tree snapshot.
    private var currentMembers: [GroupRosterMember] {
        guard model.groupRoster.groupId == groupId else { return [] }
        return model.groupRoster.members
    }

    /// Viewer membership truth, read straight from the Rust group-tree
    /// projection (`node.isMember`). The app crate derives this from the
    /// account-scoped `JoinedGroupsProjection`; the shell never scans the member
    /// roster to infer membership (D11).
    private var isCurrentMember: Bool {
        node?.isMember == true
    }

    /// Viewer admin truth, read straight from the Rust group-tree projection
    /// (`node.isAdmin`). NOT a roster scan (D11).
    private var isCurrentAdmin: Bool {
        node?.isAdmin == true
    }

    private var descendantGroupIds: Set<String> {
        func collect(from id: String, into result: inout Set<String>) {
            guard let node = model.groupTree.allNodes[id] else { return }
            for childId in node.childIds where result.insert(childId).inserted {
                collect(from: childId, into: &result)
            }
        }

        var result = Set<String>()
        collect(from: groupId, into: &result)
        return result
    }

    private var parentCandidates: [GroupParentCandidate] {
        let descendants = descendantGroupIds
        return model.groupTree.allNodes.values
            .filter { node in
                node.groupId != groupId && !descendants.contains(node.groupId)
            }
            .sorted { lhs, rhs in
                lhs.displayName.localizedCaseInsensitiveCompare(rhs.displayName) == .orderedAscending
            }
            .map { node in
                GroupParentCandidate(id: node.groupId, title: node.displayName)
            }
    }

    // Membership status derives ONLY from the JoinedGroupsProjection-backed
    // group-tree node (`node.isMember` / `node.isAdmin` / `node.isOpen`). It
    // does NOT wait on the selected-group roster snapshot. In-flight join/leave
    // transient states are NOT reconstructed in the shell from raw
    // publish-outbox kinds/tags; that pending state belongs in a typed 29er
    // domain projection.
    private var membershipStatusLabel: String {
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
        if isCurrentAdmin {
            return "shield.fill"
        }
        if isCurrentMember {
            return "checkmark.circle.fill"
        }
        return node?.isOpen == true ? "person.crop.circle.badge.plus" : "lock.fill"
    }

    private var mentionSuggestions: [GroupRosterMember] {
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
                } else if visibleMessages.isEmpty {
                    emptyChat
                } else {
                    messageStream(proxy: proxy)
                }

                if shouldShowMembershipBar {
                    membershipBar
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

                ToolbarItemGroup(placement: .navigationBarTrailing) {
                    roomToolbarActions
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
            .sheet(isPresented: $showingAdminSheet) {
                AdminActionsSheet(
                    title: title,
                    onCreateInvite: { codes in
                        model.createInvite(groupId: groupId, codes: codes)
                    },
                    onPutUser: { pubkey, role, reason in
                        model.putUser(
                            groupId: groupId,
                            targetPubkey: pubkey,
                            role: role,
                            reason: reason
                        )
                    },
                    onEditMetadata: { name, about, picture in
                        model.editGroupMetadata(
                            groupId: groupId,
                            name: name,
                            about: about,
                            picture: picture
                        )
                    },
                    onCreateChild: { localId, name, about, visibility, access in
                        model.createGroup(
                            localId: localId,
                            name: name,
                            about: about,
                            visibility: visibility,
                            access: access,
                            parent: groupId
                        )
                    },
                    parentCandidates: parentCandidates,
                    currentParentId: node?.parentId,
                    onSetParent: { parent in
                        model.setParent(groupId: groupId, parent: parent)
                    }
                )
                .presentationDetents([.large])
            }
            .task(id: groupId) {
                model.openGroupEvents(groupId)
            }
            .onChange(of: model.groupChat.messages) { _, _ in
                scrollToBottom(proxy)
            }
        }
    }

    private var emptyChat: some View {
        ContentUnavailableView(
            "No messages yet",
            systemImage: "bubble.left.and.bubble.right",
            description: Text(isCurrentMember ? "Start the conversation." : "Join this room to send the first message.")
        )
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    @ViewBuilder
    private var roomToolbarActions: some View {
        if let node {
            if !node.childIds.isEmpty {
                NavigationLink(value: GroupChildrenRoute(groupId: groupId)) {
                    Label(
                        node.childIds.count == 1 ? "1 subroom" : "\(node.childIds.count) subrooms",
                        systemImage: "bubble.left.and.bubble.right"
                    )
                }
                .labelStyle(.iconOnly)
                .accessibilityIdentifier("group-children-\(groupId)")
                .accessibilityLabel(node.childIds.count == 1 ? "1 subroom" : "\(node.childIds.count) subrooms")
            }

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

            if isCurrentAdmin {
                Button {
                    showingAdminSheet = true
                } label: {
                    Label("Admin", systemImage: "slider.horizontal.3")
                }
                .labelStyle(.iconOnly)
                .accessibilityIdentifier("admin-button-\(groupId)")
            } else if isCurrentMember {
                Button(role: .destructive) {
                    showingLeaveSheet = true
                } label: {
                    Label("Leave Group", systemImage: "person.badge.minus")
                }
                .labelStyle(.iconOnly)
                .accessibilityIdentifier("leave-button-\(groupId)")
                .accessibilityLabel("Leave Group")
            }
        }
    }

    private var shouldShowMembershipBar: Bool {
        !isCurrentMember
    }

    private var membershipBar: some View {
        HStack(spacing: 10) {
            Label(membershipStatusLabel, systemImage: membershipStatusIcon)
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .accessibilityIdentifier("membership-status-\(groupId)")

            Spacer(minLength: 8)

            if !isCurrentMember {
                Button {
                    showingJoinSheet = true
                } label: {
                    Label("Join", systemImage: "person.badge.plus")
                }
                .buttonStyle(.borderedProminent)
                .controlSize(.small)
                .accessibilityIdentifier("join-button-\(groupId)")
                .disabled(node == nil || model.kernelIsDead)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .background(Color(.systemBackground))
        .overlay(alignment: .top) { Divider() }
    }

    private func messageStream(proxy: ScrollViewProxy) -> some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 6) {
                ForEach(chatStream) { item in
                    switch item {
                    case let .dayDivider(_, label):
                        ChatDayDivider(label: label)
                            .padding(.vertical, 4)
                    case let .message(message):
                        RegistryChatMessageRow(
                            message: message,
                            wire: chatWire(for: message),
                            pending: outboxItem(for: message.id),
                            onRetry: { model.retryPublish($0) },
                            onReact: {
                                model.reactToGroupMessage(
                                    groupId: groupId,
                                    eventId: message.id,
                                    eventAuthorPubkey: message.pubkey
                                )
                            }
                        )
                    }
                }

                Color.clear
                    .frame(height: 1)
                    .id("chat-bottom")
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
        }
        .onAppear { scrollToBottom(proxy, animated: false) }
        .onChange(of: visibleMessages.count) { _, _ in
            scrollToBottom(proxy, animated: true)
        }
    }

    private func chatWire(for message: GroupChatMessage) -> NostrGroupChatMessageWire {
        NostrGroupChatMessageWire(
            id: message.id,
            authorPubkey: message.pubkey,
            content: message.copyText.isEmpty ? message.rawContent : message.copyText,
            createdAtLabel: Self.clockTime(message.createdAt),
            reactions: message.reactions.map { reaction in
                NostrGroupChatReactionWire(
                    emoji: reaction.emoji,
                    count: Int(clamping: reaction.count)
                )
            },
            isOutgoing: message.pubkey == model.activeAccountPubkey
        )
    }

    // MARK: Registry chat stream

    /// One entry in the rendered chat stream: either a day divider or a
    /// Rust-projected chat message rendered by the NMP registry row.
    private enum ChatStreamItem: Identifiable {
        case dayDivider(id: String, label: String)
        case message(GroupChatMessage)

        var id: String {
            switch self {
            case let .dayDivider(id, _): return "day-\(id)"
            case let .message(message): return "message-\(message.id)"
            }
        }
    }

    /// Fold the chronological message list into day dividers + registry rows.
    private var chatStream: [ChatStreamItem] {
        _ = model.profileRefsRevision
        var items: [ChatStreamItem] = []
        var currentDay: Int64?

        for message in visibleMessages {
            let day = Int64(message.createdAt / 86_400)
            if currentDay != day {
                items.append(.dayDivider(id: String(day), label: Self.dayLabel(message.createdAt)))
            }
            currentDay = day
            items.append(.message(message))
        }
        return items
    }

    /// Day-divider label: "Today" / "Yesterday" / "Sat, Jun 27 2026".
    private static func dayLabel(_ unixSeconds: UInt64) -> String {
        let date = Date(timeIntervalSince1970: TimeInterval(unixSeconds))
        let calendar = Calendar.current
        if calendar.isDateInToday(date) { return "Today" }
        if calendar.isDateInYesterday(date) { return "Yesterday" }
        let formatter = DateFormatter()
        formatter.setLocalizedDateFormatFromTemplate("EEE MMM d yyyy")
        return formatter.string(from: date)
    }

    /// Group-header clock time (locale short time, e.g. "10:32 AM").
    private static func clockTime(_ unixSeconds: UInt64) -> String {
        let formatter = DateFormatter()
        formatter.dateStyle = .none
        formatter.timeStyle = .short
        return formatter.string(from: Date(timeIntervalSince1970: TimeInterval(unixSeconds)))
    }

    private var composer: some View {
        VStack(spacing: 0) {
            if canCompose && !mentionSuggestions.isEmpty {
                mentionSuggestionBar
            }

            NostrGroupComposer(
                text: $draft,
                placeholder: canCompose ? "Message \(title)" : composerPromptText,
                isEnabled: canCompose,
                onSend: sendComposerText
            )
            .accessibilityIdentifier(canCompose ? "group-chat-registry-composer" : "group-chat-readonly-composer")
            .padding(.horizontal, 10)
            .padding(.vertical, 8)
            .onChange(of: draft) { _, value in
                let isTyping = canCompose && !value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                model.sendTyping(groupId: groupId, isTyping: isTyping)
            }
        }
        .background(Color(.systemBackground))
        .overlay(alignment: .top) { Divider() }
    }

    private var composerPromptText: String {
        node?.isOpen == true ? "Join to send messages" : "Invite required to send messages"
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
                            .background(Capsule().fill(Color(.tertiarySystemFill)))
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

    private func sendComposerText(_ text: String) {
        let text = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard canCompose && !text.isEmpty else { return }

        // Raw draft text + the user-picked mention pubkeys only. The shared
        // `compose_chat_message` helper in `nmp-app-29er` owns NIP-19/21
        // classification, the `@<hex>` → `nostr:npub1…` rewrite, and the
        // `["p", …]` tags. The shell does zero content tokenization. This
        // mirrors the TUI composer and Chirp's `GroupChatView.sendDraft`.
        let accepted = model.sendGroupMessage(
            groupId: groupId,
            content: text,
            mentionPubkeys: Array(selectedMentionPubkeys)
        )
        guard accepted else {
            DispatchQueue.main.async {
                draft = text
            }
            return
        }

        UIImpactFeedbackGenerator(style: .light).impactOccurred()
        selectedMentionPubkeys.removeAll()
        model.sendTyping(groupId: groupId, isTyping: false)
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

    private func acceptMention(_ member: GroupRosterMember) {
        selectedMentionPubkeys.insert(member.pubkey)
        // Insert an `@<pubkey>` *placeholder* (the raw hex identifier, NOT a
        // display name). The shared `compose_chat_message` helper in
        // `nmp-app-29er` owns the NIP-21 rewrite (`@<hex>` → `nostr:npub1…`) and
        // the `["p", …]` tag at send time. This matches the TUI composer
        // contract; the iOS shell holds zero nostr/NIP-21 knowledge.
        let mention = "@\(member.pubkey)"
        var parts = draft.split(separator: " ", omittingEmptySubsequences: false).map(String.init)
        if parts.isEmpty {
            draft = mention + " "
        } else {
            parts[parts.count - 1] = mention
            draft = parts.joined(separator: " ") + " "
        }
    }

    private func outboxItem(for eventId: String) -> PublishOutboxItem? {
        model.publishOutbox.first { $0.eventId == eventId }
    }

}

private struct GroupParentCandidate: Identifiable, Hashable {
    let id: String
    let title: String
}

private enum AdminTaskMode: String, CaseIterable, Identifiable {
    case invites
    case people
    case metadata
    case room
    case hierarchy

    var id: String { rawValue }

    var title: String {
        switch self {
        case .invites:
            return "Invites"
        case .people:
            return "People"
        case .metadata:
            return "Edit"
        case .room:
            return "Room"
        case .hierarchy:
            return "Move"
        }
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
                        .accessibilityIdentifier("join-invite-code-field")
                    TextField("Reason", text: $reason, axis: .vertical)
                        .lineLimit(2...4)
                        .accessibilityIdentifier("join-reason-field")
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
                    .accessibilityIdentifier("join-submit-button")
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
                        .accessibilityIdentifier("leave-reason-field")
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
                    .accessibilityIdentifier("leave-submit-button")
                }
            }
        }
    }
}

private struct AdminActionsSheet: View {
    let title: String
    let onCreateInvite: ([String]) -> Bool
    let onPutUser: (String, String?, String?) -> Bool
    let onEditMetadata: (String?, String?, String?) -> Bool
    let onCreateChild: (String, String, String?, String, String) -> Bool
    let parentCandidates: [GroupParentCandidate]
    let currentParentId: String?
    let onSetParent: (String?) -> Bool

    private let rootParentSelection = "__root__"

    @Environment(\.dismiss) private var dismiss
    @State private var inviteCodes = ""
    @State private var targetPubkey = ""
    @State private var role = ""
    @State private var reason = ""
    @State private var metadataName = ""
    @State private var metadataAbout = ""
    @State private var metadataPicture = ""
    @State private var childLocalId = ""
    @State private var childName = ""
    @State private var childAbout = ""
    @State private var childVisibility = "public"
    @State private var childAccess = "open"
    @State private var parentSelection = ""
    @State private var mode: AdminTaskMode = .invites
    @State private var error: String?

    private var parsedInviteCodes: [String] {
        inviteCodes
            .split { character in
                character.isWhitespace || character == ","
            }
            .map(String.init)
            .filter { !$0.isEmpty }
    }

    private var trimmedTargetPubkey: String {
        targetPubkey.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private var trimmedRole: String {
        role.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private var trimmedReason: String {
        reason.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private var trimmedMetadataName: String {
        metadataName.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private var trimmedMetadataAbout: String {
        metadataAbout.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private var trimmedMetadataPicture: String {
        metadataPicture.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private var trimmedChildLocalId: String {
        childLocalId.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private var trimmedChildName: String {
        childName.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private var trimmedChildAbout: String {
        childAbout.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Picker("Admin task", selection: $mode) {
                        ForEach(AdminTaskMode.allCases) { mode in
                            Text(mode.title).tag(mode)
                        }
                    }
                    .pickerStyle(.segmented)
                    .accessibilityIdentifier("admin-mode-picker")
                }

                switch mode {
                case .invites:
                    inviteSection
                case .people:
                    peopleSection
                case .metadata:
                    metadataSection
                case .room:
                    roomSection
                case .hierarchy:
                    hierarchySection
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
                    Button("Done") { dismiss() }
                }
            }
            .onAppear {
                if parentSelection.isEmpty {
                    parentSelection = currentParentId ?? rootParentSelection
                }
            }
        }
    }

    private var inviteSection: some View {
        Section("Invites") {
            TextField("Code", text: $inviteCodes, axis: .vertical)
                .lineLimit(1...3)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .accessibilityIdentifier("admin-invite-codes-field")

            HStack {
                Button {
                    inviteCodes = generatedInviteCode()
                } label: {
                    Label("Generate", systemImage: "wand.and.sparkles")
                }
                .accessibilityIdentifier("admin-generate-invite-button")

                Spacer()

                Button {
                    submitInvite()
                } label: {
                    Label("Create Invite", systemImage: "ticket")
                }
                .accessibilityIdentifier("admin-create-invite-button")
                .disabled(parsedInviteCodes.isEmpty)
            }
        }
    }

    private var peopleSection: some View {
        Section("People") {
            TextField("Pubkey", text: $targetPubkey)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .accessibilityIdentifier("admin-target-pubkey-field")
            TextField("Role", text: $role)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .accessibilityIdentifier("admin-role-field")
            TextField("Reason", text: $reason, axis: .vertical)
                .lineLimit(2...4)
                .accessibilityIdentifier("admin-reason-field")

            Button {
                submitPutUser()
            } label: {
                Label("Add User", systemImage: "person.badge.plus")
            }
            .accessibilityIdentifier("admin-add-user-button")
            .disabled(trimmedTargetPubkey.isEmpty)
        }
    }

    private var metadataSection: some View {
        Section("Room Metadata") {
            TextField("Name", text: $metadataName)
                .accessibilityIdentifier("admin-metadata-name-field")
            TextField("Description", text: $metadataAbout, axis: .vertical)
                .lineLimit(2...4)
                .accessibilityIdentifier("admin-metadata-about-field")
            TextField("Picture URL", text: $metadataPicture)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .keyboardType(.URL)
                .accessibilityIdentifier("admin-metadata-picture-field")

            Button {
                submitEditMetadata()
            } label: {
                Label("Update Room", systemImage: "pencil")
            }
            .accessibilityIdentifier("admin-edit-metadata-button")
            .disabled(!canSubmitEditMetadata)
        }
    }

    private var roomSection: some View {
        Section("New Room") {
            TextField("Room ID", text: $childLocalId)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .accessibilityIdentifier("admin-child-local-id-field")
            TextField("Name", text: $childName)
                .accessibilityIdentifier("admin-child-name-field")
            TextField("Description", text: $childAbout, axis: .vertical)
                .lineLimit(2...4)
                .accessibilityIdentifier("admin-child-about-field")

            Picker("Visibility", selection: $childVisibility) {
                Text("Public").tag("public")
                Text("Private").tag("private")
            }
            .pickerStyle(.segmented)
            .accessibilityIdentifier("admin-child-visibility-picker")

            Picker("Access", selection: $childAccess) {
                Text("Open").tag("open")
                Text("Closed").tag("closed")
            }
            .pickerStyle(.segmented)
            .accessibilityIdentifier("admin-child-access-picker")

            Button {
                submitCreateChild()
            } label: {
                Label("Create Room", systemImage: "plus.bubble")
            }
            .accessibilityIdentifier("admin-create-child-button")
            .disabled(trimmedChildLocalId.isEmpty || trimmedChildName.isEmpty)
        }
    }

    private var hierarchySection: some View {
        Section("Move Room") {
            Button {
                parentSelection = rootParentSelection
            } label: {
                parentCandidateLabel(title: "Root", selected: parentSelection == rootParentSelection)
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("admin-parent-option-root")

            ForEach(parentCandidates) { candidate in
                Button {
                    parentSelection = candidate.id
                } label: {
                    parentCandidateLabel(title: candidate.title, selected: parentSelection == candidate.id)
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier("admin-parent-option-\(candidate.id)")
            }

            Button {
                submitSetParent()
            } label: {
                Label(parentSelection == rootParentSelection ? "Move to Root" : "Move Room", systemImage: "arrow.triangle.branch")
            }
            .accessibilityIdentifier("admin-set-parent-button")
            .disabled(!canSubmitSetParent)
        }
    }

    private func parentCandidateLabel(title: String, selected: Bool) -> some View {
        HStack(spacing: 8) {
            Image(systemName: selected ? "checkmark.circle.fill" : "circle")
                .foregroundStyle(selected ? Color.accentColor : Color.secondary)
            Text(title)
                .foregroundStyle(.primary)
            Spacer()
        }
        .contentShape(Rectangle())
    }

    private func submitInvite() {
        let accepted = onCreateInvite(parsedInviteCodes)
        if accepted {
            inviteCodes = ""
            error = nil
        } else {
            error = "Could not create invite."
        }
    }

    private func submitPutUser() {
        let accepted = onPutUser(
            trimmedTargetPubkey,
            trimmedRole.isEmpty ? nil : trimmedRole,
            trimmedReason.isEmpty ? nil : trimmedReason
        )
        if accepted {
            targetPubkey = ""
            role = ""
            reason = ""
            error = nil
        } else {
            error = "Could not add user."
        }
    }

    private var canSubmitEditMetadata: Bool {
        !trimmedMetadataName.isEmpty
            || !trimmedMetadataAbout.isEmpty
            || !trimmedMetadataPicture.isEmpty
    }

    private func submitEditMetadata() {
        let accepted = onEditMetadata(
            trimmedMetadataName.isEmpty ? nil : trimmedMetadataName,
            trimmedMetadataAbout.isEmpty ? nil : trimmedMetadataAbout,
            trimmedMetadataPicture.isEmpty ? nil : trimmedMetadataPicture
        )
        if accepted {
            metadataName = ""
            metadataAbout = ""
            metadataPicture = ""
            error = nil
        } else {
            error = "Could not update room metadata."
        }
    }

    private func submitCreateChild() {
        let accepted = onCreateChild(
            trimmedChildLocalId,
            trimmedChildName,
            trimmedChildAbout.isEmpty ? nil : trimmedChildAbout,
            childVisibility,
            childAccess
        )
        if accepted {
            childLocalId = ""
            childName = ""
            childAbout = ""
            childVisibility = "public"
            childAccess = "open"
            error = nil
        } else {
            error = "Could not create child channel."
        }
    }

    private var canSubmitSetParent: Bool {
        let normalizedCurrent = currentParentId ?? rootParentSelection
        return !parentSelection.isEmpty && parentSelection != normalizedCurrent
    }

    private func submitSetParent() {
        let parent = parentSelection == rootParentSelection ? nil : parentSelection
        let accepted = onSetParent(parent)
        if accepted {
            error = nil
        } else {
            error = "Could not update hierarchy."
        }
    }

    private func generatedInviteCode() -> String {
        UUID().uuidString.replacingOccurrences(of: "-", with: "").lowercased().prefix(12).description
    }
}

private struct MemberListSheet: View {
    @EnvironmentObject private var model: KernelModel

    let title: String
    let members: [GroupRosterMember]

    var body: some View {
        let _ = model.profileRefsRevision
        NavigationStack {
            Group {
                if members.isEmpty {
                    ContentUnavailableView(
                        "No members",
                        systemImage: "person.2.slash",
                        description: Text("This group has not published a member list yet.")
                    )
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .background(Color(.systemGroupedBackground))
                } else {
                    ScrollView {
                        NostrGroupRosterList(participants: participants)
                            .padding(16)
                    }
                    .background(Color(.systemGroupedBackground))
                }
            }
            .navigationTitle(title)
            .navigationBarTitleDisplayMode(.inline)
        }
    }

    private var participants: [NostrGroupChatParticipantWire] {
        members.map { member in
            NostrGroupChatParticipantWire(
                pubkey: member.pubkey,
                roleLabel: member.roleBadge,
                statusLabel: member.pubkey.shortHex
            )
        }
    }
}

/// A centered day separator ("Today" / "Sat, Jun 27 2026").
private struct ChatDayDivider: View {
    let label: String

    var body: some View {
        HStack(spacing: 8) {
            VStack { Divider() }
            Text(label)
                .font(.caption2.weight(.semibold))
                .foregroundStyle(.secondary)
                .fixedSize()
            VStack { Divider() }
        }
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("chat-day-divider")
    }
}

/// Thin app chrome around the registry message row: retry/status and context
/// menu remain iOS presentation, while the chat row itself is the NMP component.
private struct RegistryChatMessageRow: View {
    let message: GroupChatMessage
    let wire: NostrGroupChatMessageWire
    let pending: PublishOutboxItem?
    let onRetry: (PublishOutboxItem) -> Void
    let onReact: () -> Void

    var body: some View {
        VStack(alignment: wire.isOutgoing ? .trailing : .leading, spacing: 3) {
            NostrGroupMessageRow(message: wire)

            if let pending {
                HStack(spacing: 6) {
                    if pending.canRetry {
                        Button { onRetry(pending) } label: {
                            Image(systemName: "arrow.clockwise.circle.fill")
                                .font(.caption.weight(.semibold))
                                .symbolRenderingMode(.hierarchical)
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel("Retry message")
                    }
                    Text(pending.status.pendingDisplayLabel)
                        .font(.caption2.weight(.medium))
                        .foregroundStyle(.tertiary)
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: wire.isOutgoing ? .trailing : .leading)
        .contentShape(Rectangle())
        .contextMenu {
            Button(action: onReact) {
                Label("React", systemImage: "heart")
            }
            Button {
                UIPasteboard.general.string = message.copyText
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
