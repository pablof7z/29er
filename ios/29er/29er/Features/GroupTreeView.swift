import SwiftUI
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

/// Label content shared by branch and leaf rows. Shows the group's display
/// name, a member-count badge, an admin-count badge, and public/closed
/// indicators. Mirrors the S02 `MainScaffold` row presentation so the
/// post-onboarding screen is visually continuous.
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

                    if node.isBranch {
                        Text("\(node.childIds.count)")
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(.white)
                            .padding(.horizontal, 7)
                            .padding(.vertical, 2)
                            .background(Capsule().fill(Color.blue))
                    }
                }

                HStack(spacing: 6) {
                    Text(subtitle)
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)

                    Spacer(minLength: 8)

                    if !node.isOpen {
                        Image(systemName: "lock.fill")
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                    }
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

    private var subtitle: String {
        var pieces: [String] = []
        if node.memberCount > 0 {
            pieces.append("\(node.memberCount) members")
        }
        if node.adminCount > 0 {
            pieces.append("\(node.adminCount) admins")
        }
        pieces.append(node.isPublic ? "public" : "private")
        if node.isBranch {
            pieces.append(node.childIds.count == 1 ? "1 room" : "\(node.childIds.count) rooms")
        }
        return pieces.joined(separator: " • ")
    }
}

/// Projection-backed destination pushed by `NavigationLink(value:)`.
struct GroupTimelineView: View {
    @EnvironmentObject private var model: KernelModel
    let groupId: String

    var body: some View {
        Group {
            if model.groupChat.messages.isEmpty {
                EmptyStateView(
                    title: "No Messages",
                    message: "No live kind:9 messages have arrived for this group yet."
                )
            } else {
                List(model.groupChat.messages) { message in
                    GroupMessageRow(message: message)
                }
                .listStyle(.plain)
            }
        }
        .navigationTitle(groupId)
        .navigationBarTitleDisplayMode(.inline)
        .task(id: groupId) {
            model.openGroupTimeline(groupId)
        }
    }
}

private struct GroupMessageRow: View {
    let message: GroupChatMessage

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 8) {
                Text(shortPubkey(message.pubkey))
                    .font(.caption.monospaced())
                    .foregroundStyle(.secondary)
                Spacer()
                Text("kind \(message.kind)")
                    .font(.caption2.monospaced())
                    .foregroundStyle(.tertiary)
            }
            Text(message.content)
                .font(.body)
            Text("created_at \(message.createdAt)")
                .font(.caption2.monospaced())
                .foregroundStyle(.tertiary)
        }
        .padding(.vertical, 6)
    }

    private func shortPubkey(_ pubkey: String) -> String {
        guard pubkey.count > 12 else { return pubkey }
        return "\(pubkey.prefix(8))…\(pubkey.suffix(4))"
    }
}
