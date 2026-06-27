import SwiftUI

@main
struct App29er: App {
    @StateObject private var model = KernelModel()

    @Environment(\.scenePhase) private var scenePhase

    var body: some Scene {
        WindowGroup {
            RootView()
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

/// S02 root router. Switches on `model.identityState`:
///   • `.signedIn`  → `MainScaffold` (authenticated app shell)
///   • `.signedOut` / `.invalidKey` / `.storageError` → `OnboardingView`
///   • `.unknown`   → loading screen until the kernel snapshot resolves it
struct RootView: View {
    @EnvironmentObject private var model: KernelModel

    var body: some View {
        switch model.identityState {
        case .signedIn:
            MainScaffold()
        case .signedOut, .invalidKey, .storageError:
            OnboardingView()
        case .unknown:
            ProgressView("Starting…")
        }
    }
}

/// S03 authenticated app shell. Per D009: a single `NavigationStack` with
/// push navigation (no `.sidebar` split column — iPhone-only). The root
/// content is `GroupTreeView`, which derives the expandable group forest
/// from `model.discoveredGroups.groups` and pushes a placeholder timeline
/// view (`TimelinePlaceholder`) for the selected group. S04 swaps the
/// placeholder for the real kind:9 timeline.
struct MainScaffold: View {
    @EnvironmentObject private var model: KernelModel

    var body: some View {
        NavigationStack {
            GroupTreeView()
                .environment(\.nostrProfileHost, model)
                .toolbar {
                    ToolbarItem(placement: .topBarTrailing) {
                        Button(role: .destructive, action: model.logout) {
                            Label("Log Out", systemImage: "rectangle.portrait.and.arrow.right")
                        }
                        .accessibilityLabel("Log Out")
                    }
                }
        }
    }
}
