import SwiftUI
import os.log

private let gtLog = Logger(subsystem: "io.f7z.app29er.bridge", category: "GroupTreeView")

/// S03 main group-tree navigation screen. Renders the Rust-owned
/// `GroupTreeSnapshot` as an expandable forest per D009:
///
///   - `NavigationStack` + push navigation (no `.sidebar` split column —
///     iPhone-only).
///   - Branch rows are `DisclosureGroup`s whose label is a `NavigationLink`
///     (D003 — branch nodes are real groups; tapping the label navigates to
///     the group's timeline, tapping the chevron expands/collapses
///     children).
///   - Leaf rows are plain `NavigationLink`s.
///   - Pushed destination is a placeholder timeline view (`TimelinePlaceholder`)
///     — S04 replaces it with the real kind:9 timeline.
///
/// Three distinct data states (T05):
///   - `isSearching && tree.roots.isEmpty` → `LoadingView`
///   - `kernelIsDead` → `ErrorStateView`
///   - otherwise empty → `EmptyStateView`
///   - otherwise → the `List` forest.
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
                List {
                    ForEach(tree.roots) { node in
                        GroupTreeRow(node: node, tree: tree)
                    }
                }
                .listStyle(.insetGrouped)
            }
        }
        .navigationTitle(navigationTitle(tree: tree))
        .navigationBarTitleDisplayMode(.inline)
        .navigationDestination(for: String.self) { groupId in
            TimelinePlaceholder(groupId: groupId)
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

/// Recursive row view. Branches render a `DisclosureGroup` with the branch's
/// own `NavigationLink` as the label (D003 — branch nodes are real groups
/// with their own timeline, not just expand/collapse folders). Leaves render
/// a plain `NavigationLink`.
struct GroupTreeRow: View {
    let node: GroupTreeNode
    let tree: GroupTreeSnapshot

    var body: some View {
        if node.isBranch {
            DisclosureGroup {
                ForEach(children) { child in
                    GroupTreeRow(node: child, tree: tree)
                }
            } label: {
                NavigationLink(value: node.groupId) {
                    GroupRowLabel(node: node)
                }
                .buttonStyle(.plain)
            }
        } else {
            NavigationLink(value: node.groupId) {
                GroupRowLabel(node: node)
            }
            .buttonStyle(.plain)
        }
    }

    private var children: [GroupTreeNode] {
        node.childIds.compactMap { tree.allNodes[$0] }
    }
}

/// Label content shared by branch and leaf rows. Shows the group's display
/// name, a member-count badge, an admin-count badge, and public/closed
/// indicators. Mirrors the S02 `MainScaffold` row presentation so the
/// post-onboarding screen is visually continuous.
struct GroupRowLabel: View {
    let node: GroupTreeNode

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(node.displayName)
                .font(.headline)
                .lineLimit(1)
            Text(node.groupId)
                .font(.caption.monospaced())
                .foregroundStyle(.secondary)
                .lineLimit(1)
            HStack(spacing: 12) {
                Label("\(node.memberCount)", systemImage: "person.2")
                Label("\(node.adminCount)", systemImage: "shield")
                if node.isPublic {
                    Label("public", systemImage: "globe")
                }
                if !node.isOpen {
                    Label("closed", systemImage: "lock")
                }
                if node.isBranch {
                    Label("\(node.childIds.count)", systemImage: "folder")
                }
            }
            .font(.caption)
            .foregroundStyle(.secondary)
        }
        .padding(.vertical, 2)
    }
}

/// Placeholder destination pushed by `NavigationLink(value:)`. S04 replaces
/// this with the real kind:9 group timeline.
struct TimelinePlaceholder: View {
    let groupId: String

    var body: some View {
        VStack(spacing: 12) {
            Image(systemName: "text.bubble")
                .font(.system(size: 48, weight: .light))
                .foregroundStyle(.secondary)
            Text("Timeline for \(groupId)")
                .font(.headline)
            Text("S04 wires the live kind:9 group timeline here.")
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .padding(.horizontal)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .navigationTitle(groupId)
        .navigationBarTitleDisplayMode(.inline)
    }
}
