import Foundation
import SwiftUI
import Combine
import os.log

private let kmLifecycleLog = Logger(subsystem: "io.f7z.app29er.bridge", category: "KernelModel")

// ── Lifecycle, liveness, open/close ──────────────────────────────────────────

@MainActor
extension KernelModel {

    /// Set the actor-death flag. Idempotent: a second call is a no-op so the
    /// foreground-resume probe and the push-side panic frame cannot
    /// double-flip.
    func markKernelDead() {
        if kernelIsDead { return }
        kmLifecycleLog.fault("kernelIsDead set — actor thread terminated")
        kernelIsDead = true
    }

    /// Probe the actor liveness through the FFI (`nmp_app_is_alive`,
    /// ADR-0028) and flip `kernelIsDead` if the actor is gone. Pulled by the
    /// `App29er` scenePhase observer on every `.active` transition.
    func checkAlive() {
        if kernelIsDead { return }
        if !kernel.isAlive() {
            markKernelDead()
        }
    }

    // ── Lifecycle ────────────────────────────────────────────────────────

    func start() {
        guard !startedKernel else { return }
        startedKernel = true
        // Seed the default relay set. Policy lives in Rust (D7): the URL + role
        // come from `nmp_app_29er::config` via the seed FFI, so the shell holds
        // no hardcoded relay literal. Swift's only job is the `NMP_TEST_RELAYS`
        // env seam (mirroring Chirp's `RelaySeeding.swift`): a well-formed
        // override seeds those relays, otherwise we fall back to the Rust
        // defaults. Seeded pre-start so `configured_relays` is populated on a
        // fresh install.
        if let testRelaysJson = ProcessInfo.processInfo.environment["NMP_TEST_RELAYS"],
           kernel.seedRelays(fromJSON: testRelaysJson) {
            // overridden for tests
        } else {
            kernel.seedDefaultRelays()
        }
        kernel.start(visibleLimit: visibleLimit, emitHz: emitHz)
        // S03 verification hook: auto-submit an nsec from the environment so
        // simulator runs can exercise the post-onboarding group tree without
        // driving the UI. Debug-only — never set in production. The nsec is
        // a throwaway generated with `nak key generate`.
        if let autoNsec = ProcessInfo.processInfo.environment["S03_AUTO_SIGN_IN_NSEC"],
           autoNsec.hasPrefix("nsec1") {
            submitNsec(autoNsec)
        }
    }

    func stop() {
        kernel.stop()
        startedKernel = false
    }

    func resetAndRestart() {
        kernel.reset()
        // Clear every typed projection slot so the computed accessors collapse
        // to their empty defaults. The next post-reset tick reassigns them
        // unconditionally.
        clearTypedProjections()
        kernel.lastAppliedRev = 0
        lastErrorToast = nil
        lastErrorCategory = nil
        kernel.start(visibleLimit: visibleLimit, emitHz: emitHz)
        startedKernel = true
    }

    /// Reset the local kernel state and restart, then reopen group discovery.
    /// Wired to the Settings → "Reset Local Database" action. Uses the Rust
    /// object's `reset()` (transient kernel state) + a re-seed + restart, then
    /// `refreshGroupDiscovery` to reopen the discover session against the
    /// active relay. The saved account stays in Keychain (the keyring
    /// capability is untouched).
    func resetLocalDatabaseAndRestart() {
        let relayToRefresh = relaySelector.activeRelayUrl
        kernel.reset()
        // Clear every typed projection slot so views immediately leave stale
        // rows behind while the reset runtime restarts.
        clearTypedProjections()
        kernel.lastAppliedRev = 0
        selectedGroupId = nil
        lastErrorToast = nil
        lastErrorCategory = nil
        if let testRelaysJson = ProcessInfo.processInfo.environment["NMP_TEST_RELAYS"],
           kernel.seedRelays(fromJSON: testRelaysJson) {
            // overridden for tests
        } else {
            kernel.seedDefaultRelays()
        }
        kernel.start(visibleLimit: visibleLimit, emitHz: emitHz)
        startedKernel = true
        discoveredGroups.refreshSessionAfterLocalDatabaseReset(relayUrl: relayToRefresh)
    }

    /// Open NIP-29 group discovery for `hostRelayUrl` (the read side of the
    /// discover screen). Delegates to `DiscoveredGroupsStore.searchGroups`
    /// which opens the read projection + dispatches the `nmp.nip29.discover`
    /// action. Callers pass `groupDefaults.suggestedRelayUrl` (the Rust-owned
    /// default) or a user-entered relay — never a Swift literal (D7).
    func openGroupDiscovery(hostRelayUrl: String) {
        discoveredGroups.searchGroups(relayUrl: hostRelayUrl)
    }

    /// Close the current NIP-29 group-discovery session (if any). Tears down
    /// the read projection so no stale group catalog is emitted after the
    /// discover screen is dismissed.
    func closeGroupDiscovery() {
        discoveredGroups.closeSession()
    }

    // ── S02 identity ─────────────────────────────────────────────────────

    /// Submit an nsec for sign-in. Performs a quick client-side format check
    /// (starts with `nsec1`, length >= 40) before dispatching to Rust; a
    /// malformed nsec flips `identityState` to `.invalidKey` and returns
    /// without dispatching (D004 — the nsec never reaches NMP). A valid-looking
    /// nsec flips `identityState` to `.unknown` (loading) and dispatches
    /// `nmp_app_signin_nsec` to the actor; the next `KACT` tick flips the
    /// state to `.signedIn(pubkey)` on success or `.signedOut` on rejection
    /// (the `active_account` slot stays nil).
    ///
    /// The nsec string is cleared from this stack frame immediately after
    /// dispatch — Swift never holds the nsec beyond the dispatch moment (D004).
    func submitNsec(_ nsec: String) {
        let trimmed = nsec.trimmingCharacters(in: .whitespacesAndNewlines)
        // Quick client-side format check. The authoritative validation is
        // `nostr::Keys::parse` in Rust; this is a fast-fail so a typo does
        // not round-trip through the actor.
        guard trimmed.hasPrefix("nsec1"), trimmed.count >= 40 else {
            identityState = .invalidKey
            return
        }
        // Loading state — the next `KACT` tick resolves this to `.signedIn`
        // or `.signedOut` (see `apply`).
        identityState = .unknown
        // D004 — hand the nsec to NMP once. The nsec is never stored on the
        // model; `trimmed` is a local that is released when this frame
        // returns, so Swift does not hold the nsec beyond the dispatch.
        kernel.signInNsec(trimmed)
    }

    func logout() {
        guard let pubkey = activeAccountPubkey else { return }
        kernel.removeAccount(pubkey)
    }

    func retryPublish(_ item: PublishOutboxItem) {
        guard item.canRetry else { return }
        kernel.retryPublish(handle: item.handle)
    }
}
