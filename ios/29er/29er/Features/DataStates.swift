import SwiftUI

/// Reusable data-state views for the three distinct empty/loading/error
/// states a feature screen can be in. Wire into `GroupTreeView` and any
/// future feature screen so the states are visually consistent across the
/// app (D009 pattern — single source of truth for "loading", "empty",
/// "couldn't load").

/// Indeterminate loading state. Centered `ProgressView` plus a secondary
/// label describing what is in flight.
struct LoadingView: View {
    var label: String = "Loading…"

    var body: some View {
        VStack(spacing: 12) {
            ProgressView()
            Text(label)
                .font(.subheadline)
                .foregroundStyle(.secondary)
        }
        .padding(24)
        .frame(maxWidth: 340)
        .glassPanel(cornerRadius: 22)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(.systemGroupedBackground))
    }
}

/// Empty state — no data yet, but the relay/kernel is reachable and a fresh
/// snapshot just has nothing to show. Uses `ContentUnavailableView` for the
/// iOS-native "nothing here" presentation.
struct EmptyStateView: View {
    let title: String
    let message: String
    var systemImage: String = "rectangle.stack"

    var body: some View {
        ContentUnavailableView(
            title,
            systemImage: systemImage,
            description: Text(message)
        )
        .padding(24)
        .frame(maxWidth: 360)
        .glassPanel(cornerRadius: 22)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(.systemGroupedBackground))
    }
}

/// Error / couldn't-load state. The relay is unreachable, the kernel is
/// dead, or a decode failure tripped. Distinct from `EmptyStateView` so the
/// user can tell "nothing to show" apart from "something broke".
struct ErrorStateView: View {
    let message: String
    var title: String = "Couldn't Load"
    var systemImage: String = "wifi.exclamationmark"

    var body: some View {
        ContentUnavailableView(
            title,
            systemImage: systemImage,
            description: Text(message)
        )
        .padding(24)
        .frame(maxWidth: 360)
        .glassPanel(cornerRadius: 22)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(.systemGroupedBackground))
    }
}

extension View {
    func glassPanel(cornerRadius: CGFloat = 18, interactive: Bool = false) -> some View {
        let effect = interactive ? Glass.regular.interactive() : Glass.regular
        return self.glassEffect(effect, in: .rect(cornerRadius: cornerRadius))
    }
}
