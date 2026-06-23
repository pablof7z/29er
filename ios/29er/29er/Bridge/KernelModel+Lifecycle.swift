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
        // 29er's hardcoded default relay for M001 (R002): nip29.f7z.io.
        // Policy lives in Rust (the canonical NMP composition wired by
        // `nmp_app_29er_register`), but the bootstrap relay is a 29er product
        // decision — surfaced here so the shell keeps a single explicit
        // default (mirroring Chirp's `RelaySeeding.swift` posture).
        kernel.addRelay(url: "wss://nip29.f7z.io", role: "outbox")
        kernel.start(visibleLimit: visibleLimit, emitHz: emitHz)
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

    /// Open NIP-29 group discovery for `hostRelayUrl` (the read side of the
    /// discover screen). Delegates to `DiscoveredGroupsStore.searchGroups`
    /// which opens the read projection + dispatches the `nmp.nip29.discover`
    /// action. T06's `ShakeoutView` calls this with `wss://nip29.f7z.io`.
    func openGroupDiscovery(hostRelayUrl: String) {
        discoveredGroups.searchGroups(relayUrl: hostRelayUrl)
    }

    /// Close the current NIP-29 group-discovery session (if any). Tears down
    /// the read projection so no stale group catalog is emitted after the
    /// discover screen is dismissed.
    func closeGroupDiscovery() {
        discoveredGroups.closeSession()
    }
}