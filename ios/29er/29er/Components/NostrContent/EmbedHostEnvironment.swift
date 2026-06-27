import SwiftUI

/// Read-only source of resolved embed envelopes the kernel pushes on every
/// snapshot frame, keyed by `primaryId` (event-id hex or `kind:pubkey:d`
/// coordinate). The app binds a concrete conformer (holding the typed
/// `refs.event` sidecar map) into the environment via
/// `.embedEnvelopeSource(...)`; `NostrContentView`'s event-ref renderer reads
/// it to feed `EmbeddedEvent`.
///
/// THIN-SHELL: the conformer only stores and looks up the already-resolved
/// envelopes — no kind dispatch, no protocol parsing (the Rust resolver did
/// that before the envelope crossed the wire).
@MainActor
public protocol EmbedEnvelopeSource {
    func envelopeForPrimaryID(_ id: String) -> EmbeddedEventEnvelope?
    func envelopeForURI(_ uri: String) -> EmbeddedEventEnvelope?
}

// MARK: - Environment keys

// The `EnvironmentKey.defaultValue` requirement is `nonisolated`; these defaults
// are `nil` (no shared mutable state), so `nonisolated(unsafe)` is sound and
// keeps the keys usable under Swift 6 strict concurrency without forcing the
// whole app into default main-actor isolation.
private struct EmbedEnvelopeSourceKey: EnvironmentKey {
    nonisolated(unsafe) static let defaultValue: EmbedEnvelopeSource? = nil
}

private struct EmbedClaimSinkKey: EnvironmentKey {
    nonisolated(unsafe) static let defaultValue: EventClaimSinkProtocol? = nil
}

private struct NostrKindRegistryKey: EnvironmentKey {
    nonisolated(unsafe) static let defaultValue: NostrKindRegistry? = nil
}

public extension EnvironmentValues {
    /// The host that resolves embed envelopes for `nostr:` event refs.
    var embedEnvelopeSource: EmbedEnvelopeSource? {
        get { self[EmbedEnvelopeSourceKey.self] }
        set { self[EmbedEnvelopeSourceKey.self] = newValue }
    }

    /// The resolve/release sink `EmbeddedEvent` fires on enter/exit so the
    /// kernel reference-counts the embed URI through `refs.event` and triggers
    /// upstream fetch through the real event-ref adapter.
    var embedClaimSink: EventClaimSinkProtocol? {
        get { self[EmbedClaimSinkKey.self] }
        set { self[EmbedClaimSinkKey.self] = newValue }
    }

    /// The kind → renderer dispatch table consulted for each resolved embed.
    var nostrKindRegistry: NostrKindRegistry? {
        get { self[NostrKindRegistryKey.self] }
        set { self[NostrKindRegistryKey.self] = newValue }
    }
}

public extension View {
    /// Bind the embed host, claim sink, and kind registry so any nested
    /// `NostrContentView` renders `nostr:` event refs through the kind-dispatch
    /// registry (ADR-0034).
    func embedEnvelopeSource(
        _ source: EmbedEnvelopeSource?,
        claimSink: EventClaimSinkProtocol? = nil,
        registry: NostrKindRegistry? = nil
    ) -> some View {
        self
            .environment(\.embedEnvelopeSource, source)
            .environment(\.embedClaimSink, claimSink)
            .environment(\.nostrKindRegistry, registry)
    }
}
