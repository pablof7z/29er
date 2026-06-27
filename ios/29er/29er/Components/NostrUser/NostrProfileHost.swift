import SwiftUI

/// Host bridge for profile projections owned by the NMP kernel.
///
/// Registry components call this bridge with stable Nostr references. The app
/// supplies the platform adapter; the component owns when to resolve, release,
/// and re-read the current projection.
@MainActor
public protocol NostrProfileHost: AnyObject {
    func profile(forPubkey pubkey: String) -> ProfileWire?
    func resolveProfileRef(pubkey: String, consumerID: String)
    func releaseProfileRef(pubkey: String, consumerID: String)
}

private struct NostrProfileHostKey: EnvironmentKey {
    nonisolated(unsafe)
    static let defaultValue: NostrProfileHost? = nil
}

public extension EnvironmentValues {
    var nostrProfileHost: NostrProfileHost? {
        get { self[NostrProfileHostKey.self] }
        set { self[NostrProfileHostKey.self] = newValue }
    }
}
