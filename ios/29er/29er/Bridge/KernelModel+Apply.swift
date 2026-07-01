import Foundation
import os.log

private let kmApplyLog = Logger(subsystem: "io.f7z.app29er.bridge", category: "KernelModel")

// ── Snapshot apply ────────────────────────────────────────────────────────────

@MainActor
extension KernelModel {

    func apply(result: KernelUpdateResult) {
        // Staleness guard on the bare envelope `rev`. Production always emits
        // `rev` on every frame, so a zero rev is not a valid production frame
        // and is dropped.
        guard result.rev > kernel.lastAppliedRev else { return }

        // 29er S01 consumes only the discovered-groups + active-account
        // sidecars. The Chirp pattern assigns ONLY the @Published slots whose
        // projection key advanced in this frame (`result.changedKeys`); 29er
        // does not yet wire the ProjectionMergeCache (single snapshot key, no
        // rev-aware cache needs in S01), so we assign unconditionally — the
        // kernel emits a full frame on every tick. When 29er grows to consume
        // more projections, wire the cache-merge layer + the `changedKeys` gate
        // (mirroring Chirp's `KernelModel+Apply.swift`).
        typedDiscoveredGroups = result.typedDiscoveredGroups
        typedGroupTree = result.typedGroupTree
        typedGroupChat = result.typedGroupChat
        typedGroupRoster = result.typedGroupRoster
        typedPublishOutbox = result.typedPublishOutbox
        typedActiveAccount = result.typedActiveAccount
        typedGroupDefaults = result.typedGroupDefaults
        if let refsProfile = result.typedProjectionPayload(key: "refs.profile"),
           profileRefs.merge(
               payload: refsProfile,
               sessionId: result.sessionId,
               snapshotEpoch: result.snapshotEpoch
           ) {
            profileRefsRevision &+= 1
        }
        if let refEvents = result.typedProjectionPayload(key: TypedRefEventEnvelopesDecoder.key),
           eventEnvelopes.replace(payload: refEvents) {
            eventRefsRevision &+= 1
        }
        typedRelaySelector = result.typedRelaySelector
        typedRelayDiagnostics = result.typedRelayDiagnostics

        // S02 — derive `identityState` from the `active_account` typed
        // projection. The first tick with `rev > 0` collapses `unknown` to
        // either `signedIn(pubkey)` or `signedOut`; a subsequent sign-in
        // flips `unknown` (set by `submitNsec`) to `signedIn` once the
        // `KACT` sidecar carries the pubkey. `invalidKey` is a client-side
        // state set by `submitNsec`'s format check and is only collapsed
        // here once a real tick arrives (so a rejected nsec does not get
        // silently cleared by a stale snapshot).
        if result.rev > 0 {
            // Capture pre-apply state so we can detect the signedIn transition.
            let wasSignedIn: Bool
            if case .signedIn = identityState { wasSignedIn = true } else { wasSignedIn = false }

            if let pubkey = result.typedActiveAccount, !pubkey.isEmpty {
                identityState = .signedIn(pubkey: pubkey)
            } else if identityState != .invalidKey {
                identityState = .signedOut
            }

            // Auto-open group discovery on the first signedIn tick — covers
            // both fresh sign-in AND session restore via keychain. Fires once:
            // `wasSignedIn` gates the transition tick, and `hostRelayUrl` being
            // non-empty after the first call prevents re-entry. GroupTreeView's
            // .task guard also deduplicates when the view appears later.
            //
            // The host relay comes from the Rust-owned `group_defaults`
            // projection (D7) — never a Swift literal. `group_defaults` is a
            // static snapshot registered at app init, so it is present on this
            // tick; if it has not landed yet the guard skips and the next tick
            // (or GroupTreeView's `.task`) retries.
            let suggestedRelay = relaySelector.activeRelayUrl
            if !wasSignedIn, case .signedIn = identityState,
               discoveredGroups.hostRelayUrl.isEmpty, !suggestedRelay.isEmpty {
                openGroupDiscovery(hostRelayUrl: suggestedRelay)
            }
            if case .signedIn = identityState,
               !relaySelector.activeRelayUrl.isEmpty,
               discoveredGroups.hostRelayUrl != relaySelector.activeRelayUrl {
                openGroupDiscovery(hostRelayUrl: relaySelector.activeRelayUrl)
            }
        }

        // Snapshot-driven error toast (tap-to-dismiss has nowhere else to
        // land, so it stays a distinct slot from any user-clearable toast).
        lastErrorToast = result.lastErrorToast
        lastErrorCategory = result.lastErrorCategory

        // NIP-29 group-discovery projection mirror. Push every tick so the
        // store tracks `projections["nmp.nip29.discovered_groups"]`. The store
        // is unwired until the user enters a relay and taps Search
        // (`searchGroups`); the snapshot key is `nil` until then, and the
        // store ignores stale snapshots from a previously-registered relay
        // during a switch.
        discoveredGroups.apply(snapshot: result.typedDiscoveredGroups)
        if let selectedGroupId {
            discoveredGroups.markGroupRead(groupId: selectedGroupId)
        }

        kmApplyLog.debug(
            "NMP_PERF swift_apply rev=\(result.rev, privacy: .public) payload_bytes=\(result.payloadBytes, privacy: .public) decode_us=\(result.decodeMicros, privacy: .public)")

        kernel.lastAppliedRev = result.rev
        snapshotCount &+= 1
        lastSnapshotAt = Date()
    }

    /// The authoritative snapshot revision. Reads the last-applied `rev`
    /// (mirrors Chirp's `rev` accessor reading the typed envelope). `0` before
    /// the first tick lands.
    var rev: UInt64 { kernel.lastAppliedRev }

    /// Null every typed projection slot so the computed accessors collapse to
    /// their empty defaults. Used by `resetAndRestart()`: the next tick
    /// reassigns each slot, so this is a transient blank, not a steady state.
    func clearTypedProjections() {
        typedDiscoveredGroups = nil
        typedGroupTree = nil
        typedGroupChat = nil
        typedGroupRoster = nil
        typedPublishOutbox = nil
        typedActiveAccount = nil
        typedGroupDefaults = nil
        profileRefs.reset()
        profileRefsRevision &+= 1
        eventEnvelopes.reset()
        eventRefsRevision &+= 1
        typedRelaySelector = nil
        typedRelayDiagnostics = nil
    }

    /// Active account pubkey (`nil` ⇒ no active account). Read through the
    /// `typedActiveAccount` slot.
    var activeAccountPubkey: String? {
        typedActiveAccount
    }

    /// Discovered groups snapshot (`nil` ⇒ discovery not registered or last
    /// tick's sidecar was malformed). Read through the
    /// `typedDiscoveredGroups` slot.
    var discoveredGroupsSnapshot: DiscoveredGroupsSnapshot? {
        typedDiscoveredGroups
    }

    var groupTree: GroupTreeSnapshot {
        typedGroupTree ?? .empty
    }

    var groupChat: GroupChatSnapshot {
        typedGroupChat ?? .empty
    }

    var groupRoster: GroupRosterSnapshot {
        typedGroupRoster ?? .empty
    }

    var publishOutbox: [PublishOutboxItem] {
        typedPublishOutbox ?? []
    }

    /// NIP-29 group-create defaults (`nil` slot ⇒ `.empty`). Carries 29er's
    /// Rust-owned suggested public-group host relay (`suggestedRelayUrl`); the
    /// shell reads it instead of hardcoding a relay URL (D7).
    var groupDefaults: GroupDefaultsSnapshot {
        typedGroupDefaults ?? .empty
    }

    var relaySelector: RelaySelectorSnapshot {
        typedRelaySelector ?? .empty
    }

    var relayDiagnostics: RelayDiagnosticsSnapshot {
        typedRelayDiagnostics ?? .empty
    }

    var activeRelayTitle: String {
        relayDisplayName(for: relaySelector.activeRelayUrl)
    }

    func relayDisplayName(for relayUrl: String) -> String {
        if let name = relayDiagnostics.relay(for: relayUrl)?.nip11Name, !name.isEmpty {
            return name
        }
        return relayUrl.relayHostLabel.isEmpty ? "Relay" : relayUrl.relayHostLabel
    }
}

@MainActor
extension KernelUpdateResult {
    func typedProjectionPayload(key: String) -> Data? {
        typedProjections.first(where: { $0.key == key })?.payload
    }
}
