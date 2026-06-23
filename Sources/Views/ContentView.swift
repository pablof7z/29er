import SwiftUI

struct ContentView: View {
    @EnvironmentObject var model: KernelModel
    @State private var showNsecInput = false

    var body: some View {
        if model.nsec.isEmpty {
            SignInView(showSignIn: $showNsecInput)
                .environmentObject(model)
        } else {
            MainView()
                .environmentObject(model)
        }
    }
}

struct MainView: View {
    @EnvironmentObject var model: KernelModel

    var body: some View {
        HStack(spacing: 0) {
            SidebarView()
                .environmentObject(model)
                .frame(maxWidth: 300)

            Divider()

            if let selectedGroup = model.selectedGroup {
                ChatView(group: selectedGroup)
                    .environmentObject(model)
            } else {
                EmptyStateView()
            }
        }
    }
}

struct SidebarView: View {
    @EnvironmentObject var model: KernelModel
    @State private var expandedGroups = Set<String>()

    var body: some View {
        VStack(spacing: 0) {
            VStack(alignment: .leading, spacing: 12) {
                Text("29er")
                    .font(.system(size: 24, weight: .bold))
                Text("Connected to nip29.f7z.io")
                    .font(.caption)
                    .foregroundColor(.secondary)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding()

            if model.isLoadingGroups {
                ProgressView()
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .center)
            } else if model.groupTree.isEmpty {
                Text("No groups found")
                    .foregroundColor(.secondary)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .center)
            } else {
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 0) {
                        ForEach(model.groupTree, id: \.id) { group in
                            GroupRowView(
                                group: group,
                                depth: 0,
                                expandedGroups: $expandedGroups,
                                onSelect: { model.selectGroup(group) }
                            )
                        }
                    }
                    .padding(.vertical, 8)
                }
            }

            Divider()

            Button(action: { /* Sign out */ }) {
                Label("Sign Out", systemImage: "rectangle.portrait.and.arrow.right")
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding()
        }
        .background(Color(.systemBackground))
        .frame(maxHeight: .infinity, alignment: .topLeading)
        .task {
            await model.fetchGroupTree()
        }
    }
}

struct GroupRowView: View {
    let group: GroupNode
    let depth: Int
    @Binding var expandedGroups: Set<String>
    let onSelect: () -> Void
    @EnvironmentObject var model: KernelModel

    private var isExpanded: Bool { expandedGroups.contains(group.id) }
    private var hasChildren: Bool { !group.children.isEmpty }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 8) {
                if hasChildren {
                    Button(action: toggle) {
                        Image(systemName: isExpanded ? "chevron.down" : "chevron.right")
                            .frame(width: 16)
                    }
                    .buttonStyle(.plain)
                } else {
                    Spacer().frame(width: 16)
                }

                Button(action: onSelect) {
                    HStack(spacing: 12) {
                        Circle()
                            .fill(Color.blue.opacity(0.3))
                            .frame(width: 32, height: 32)
                            .overlay(
                                Text(String(group.name.prefix(1)))
                                    .font(.caption.bold())
                            )

                        VStack(alignment: .leading, spacing: 2) {
                            Text(group.name)
                                .font(.callout)
                                .lineLimit(1)
                            if group.lastActivityAt > .distantPast {
                                Text(group.lastActivityAt.formatted(.relative(presentation: .named)))
                                    .font(.caption2)
                                    .foregroundColor(.secondary)
                            }
                        }

                        Spacer()
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .foregroundColor(.primary)
                .background(
                    model.selectedGroup?.id == group.id ?
                    Color.blue.opacity(0.15) : Color.clear
                )
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 8)

            if isExpanded && hasChildren {
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(group.children, id: \.id) { child in
                        GroupRowView(
                            group: child,
                            depth: depth + 1,
                            expandedGroups: $expandedGroups,
                            onSelect: { model.selectGroup(child) }
                        )
                    }
                }
                .padding(.leading, 12)
            }
        }
    }

    private func toggle() {
        if isExpanded {
            expandedGroups.remove(group.id)
        } else {
            expandedGroups.insert(group.id)
        }
    }
}

struct ChatView: View {
    let group: GroupNode
    @EnvironmentObject var model: KernelModel
    @State private var messageText = ""
    @State private var showMemberList = false

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                VStack(alignment: .leading) {
                    Text(group.name)
                        .font(.headline)
                }
                Spacer()
                Button(action: { showMemberList.toggle() }) {
                    Label("Members", systemImage: "person.2")
                }
            }
            .padding()
            .background(Color(.systemBackground))

            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    if model.messages.isEmpty {
                        Text("No messages yet")
                            .foregroundColor(.secondary)
                            .frame(maxWidth: .infinity, alignment: .center)
                            .padding()
                    } else {
                        ForEach(model.messages) { message in
                            MessageRowView(message: message)
                        }
                    }
                }
                .padding()
            }

            Divider()

            HStack(spacing: 12) {
                TextField("Message", text: $messageText)
                    .textFieldStyle(.roundedBorder)

                Button(action: {
                    Task {
                        await model.postMessage(messageText, mentions: [])
                        messageText = ""
                    }
                }) {
                    Image(systemName: "arrow.up.circle.fill")
                        .font(.title3)
                }
                .disabled(messageText.trimmingCharacters(in: .whitespaces).isEmpty)
            }
            .padding()
        }
        .sheet(isPresented: $showMemberList) {
            MemberListView()
                .environmentObject(model)
        }
    }
}

struct MessageRowView: View {
    let message: Message

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                Text(message.author)
                    .font(.caption.bold())
                Spacer()
                Text(message.timestamp.formatted(date: .omitted, time: .shortened))
                    .font(.caption)
                    .foregroundColor(.secondary)
            }
            Text(message.content)
                .lineLimit(nil)
        }
        .padding(12)
        .background(Color(.systemGray5))
        .cornerRadius(8)
    }
}

struct MemberListView: View {
    @EnvironmentObject var model: KernelModel

    var body: some View {
        NavigationStack {
            List(model.currentGroupMembers) { member in
                HStack {
                    Circle()
                        .fill(Color.blue.opacity(0.3))
                        .frame(width: 32, height: 32)
                        .overlay(
                            Text(String(member.name?.prefix(1) ?? "?"))
                                .font(.caption.bold())
                        )
                    VStack(alignment: .leading) {
                        Text(member.name ?? "Unknown")
                        Text(member.pubkey.prefix(12) + "...")
                            .font(.caption)
                            .foregroundColor(.secondary)
                    }
                }
            }
            .navigationTitle("Members")
            .navigationBarTitleDisplayMode(.inline)
        }
    }
}

struct EmptyStateView: View {
    var body: some View {
        VStack(spacing: 12) {
            Image(systemName: "bubble.right")
                .font(.system(size: 48))
                .foregroundColor(.gray)
            Text("Select a group to start chatting")
                .font(.headline)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(.systemBackground))
    }
}

struct SignInView: View {
    @EnvironmentObject var model: KernelModel
    @Binding var showSignIn: Bool
    @State private var nsecInput = ""

    var body: some View {
        VStack(spacing: 24) {
            Spacer()

            VStack(spacing: 12) {
                Text("29er")
                    .font(.system(size: 48, weight: .bold))
                Text("Nostr Groups")
                    .font(.subheadline)
                    .foregroundColor(.secondary)
            }

            VStack(spacing: 12) {
                Text("Enter your Nostr private key (nsec) to sign in")
                    .foregroundColor(.secondary)

                TextField("nsec...", text: $nsecInput)
                    .textFieldStyle(.roundedBorder)
                    .font(.system(.body, design: .monospaced))

                Button(action: {
                    Task {
                        await model.signIn(with: nsecInput)
                    }
                }) {
                    Text("Sign In")
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderedProminent)
                .disabled(nsecInput.isEmpty)
            }
            .padding()

            Spacer()
        }
        .padding()
    }
}

#Preview {
    ContentView()
        .environmentObject(KernelModel())
}
