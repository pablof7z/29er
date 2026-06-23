import SwiftUI

@main
struct App29er: App {
    @StateObject private var model = KernelModel()

    @Environment(\.scenePhase) private var scenePhase

    var body: some Scene {
        WindowGroup {
            ShakeoutView()
                .environmentObject(model)
                .task {
                    // Skip kernel boot when the app is launched as an XCTest
                    // host. Starting the kernel here saturates the main thread
                    // with the 4Hz snapshot→@MainActor apply storm, which
                    // starves the XCTest runner during its "preparing to run
                    // tests" phase and trips the runner-prepare timeout. Unit
                    // tests construct their own `KernelModel()` and drive it
                    // directly, so the host runtime is unnecessary under test.
                    if ProcessInfo.processInfo.environment["XCTestConfigurationFilePath"] == nil {
                        model.start()
                    }
                }
        }
        .onChange(of: scenePhase) { _, newPhase in
            // D7: Swift reports the fact; the kernel decides what each phase
            // MEANS (reconcile NIP-77 watermarks on Bg→Fg, throttle retries
            // on Fg→Bg, etc.). No policy lives here.
            switch newPhase {
            case .active:
                // ADR-0028: pull-side actor-liveness probe. If the app was
                // backgrounded across an actor panic, the push-side panic
                // frame may have arrived and the Swift listener thread may
                // have already exited before the host had a chance to react.
                // Probing here on every foreground transition catches the
                // missed signal. Probe BEFORE `lifecycleForeground` so a dead
                // kernel does not also get hit with a doomed lifecycle command.
                model.checkAlive()
                model.kernel.lifecycleForeground()
            case .background:
                model.kernel.lifecycleBackground()
            case .inactive:
                break // transient — kernel never hears about it.
            @unknown default:
                break
            }
        }
    }
}

/// S01/T06 shakeout proof: opens group discovery on `wss://nip29.f7z.io` and
/// renders the live `DiscoveredGroupsProjection` count + first few rows.
/// Proves SwiftUI observes the same typed sidecar the Rust CLI proved
/// against the live relay (S01 gate criterion 3).
struct ShakeoutView: View {
    @EnvironmentObject private var model: KernelModel

    var body: some View {
        NavigationStack {
            Group {
                let groups = model.discoveredGroups.groups
                if model.discoveredGroups.isSearching && groups.isEmpty {
                    ProgressView("Discovering groups on nip29.f7z.io…")
                } else if groups.isEmpty {
                    ContentUnavailableView(
                        "No Groups",
                        systemImage: "rectangle.stack",
                        description: Text("Discovery has not returned any groups yet.")
                    )
                } else {
                    List(groups.prefix(50)) { group in
                        VStack(alignment: .leading, spacing: 4) {
                            Text(group.displayName)
                                .font(.headline)
                            Text(group.groupId)
                                .font(.caption.monospaced())
                                .foregroundStyle(.secondary)
                            HStack(spacing: 12) {
                                Label("\(group.memberCount)", systemImage: "person.2")
                                Label("\(group.adminCount)", systemImage: "shield")
                                if group.public {
                                    Label("public", systemImage: "globe")
                                }
                                if !group.open {
                                    Label("closed", systemImage: "lock")
                                }
                            }
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        }
                        .padding(.vertical, 2)
                    }
                }
            }
            .navigationTitle("29er · \(model.discoveredGroups.groups.count) groups")
            .navigationBarTitleDisplayMode(.inline)
        }
        .task {
            model.openGroupDiscovery(hostRelayUrl: "wss://nip29.f7z.io")
        }
    }
}