import Foundation
import os.log

let kbLog = Logger(subsystem: "io.f7z.app29er.bridge", category: "KernelBridge")

/// Mirror of `KERNEL_SCHEMA_VERSION` (Rust: `crates/nmp-core/src/update_envelope.rs`).
/// Must be bumped in lock-step when the Rust constant changes. A mismatch
/// causes `KernelHandle.decode(frame:)` to reject the snapshot rather than
/// silently misparse renamed or retyped fields.
private let KERNEL_SCHEMA_VERSION: UInt32 = 1

/// Thin Swift bridge over the generated `TwentyNinerApp` UniFFI object.
///
/// Clean-break: the hand-written C-ABI (`NmpCore.h` + `nmp_app_*`) is gone.
/// Composition (substrate + NIP-29 actions + group-create defaults + the
/// relay selector) now happens in Rust inside `TwentyNinerApp()` — there is no
/// register / declare / broker dance left in Swift. `KernelHandle` keeps the
/// same method names the rest of the shell already calls so the bridge swap is
/// local: each method now forwards to a typed `TwentyNinerApp` method instead
/// of a raw C symbol. The NIP-29 group-discovery / group-chat / dispatch verbs
/// live on the `KernelHandle` extension in `GroupDiscoveryBridge.swift`.
final class KernelHandle {
    /// The 29er UniFFI runtime object. Owns the composed `nmp-native-runtime`
    /// app; Arc-shared with Rust.
    let app: TwentyNinerApp

    /// Retained update sink registered via `set_update_sink`. Held so it
    /// outlives the registration; cleared on re-`listen()` (replace) or
    /// `deinit`. UniFFI owns its own reference across the FFI; this keeps the
    /// Swift-side strong ref alive for as long as it is registered.
    private var updateSink: KernelUpdateSink?

    /// Strong reference to the native keyring capability handler. Held so the
    /// sink stays alive for the kernel lifetime (it is registered before
    /// `start()` and the actor may call back into it at any tick).
    private var capabilities: TwentyNinerCapabilities?

    /// Last-applied snapshot revision. Mutated by `KernelModel.apply` on
    /// `@MainActor`. Read by the staleness guard. Not `@Published` — `rev` is
    /// not a view-facing value. Lives on the handle so extensions can read/write
    /// it without a stored property in an extension (illegal in Swift).
    var lastAppliedRev: UInt64 = 0

    init() {
        // Construct + compose 29er in Rust. No IO; the actor is NOT started.
        app = TwentyNinerApp()
        Self.configureStoragePath(for: app)
        // ADR-0053 — 29er is a full client: declare that it consumes every
        // kernel-owned built-in Tier-2 projection. Must run before `start`; the
        // kernel narrows its built-in output to this declaration.
        app.declareConsumedProjections()
        // ADR-0055 Rung 3 (incremental apply) is intentionally NOT declared: it
        // makes the kernel omit `Unchanged` typed projections, which requires the
        // host to retain a per-key merge cache. 29er's `KernelModel.apply`
        // assigns every typed slot unconditionally from the current frame and
        // does NOT yet wire the ProjectionMergeCache (see `KernelModel+Apply`),
        // so it relies on the kernel emitting a full frame every tick. Declaring
        // incremental apply here would let the kernel drop the unchanged
        // `active_account` (`KACT`) sidecar after the sign-in tick, clobbering
        // `typedActiveAccount` to nil and collapsing `identityState` back to
        // `.signedOut` — the user gets stuck on onboarding. Wire the merge cache
        // (mirroring Chirp) before opting back into Rung 3.
        // Register the native keyring capability handler BEFORE `start` so the
        // kernel can route capability requests from the first tick (identity
        // restore reads from Keychain during startup). Held by `capabilities`
        // for the kernel lifetime.
        let capabilities = TwentyNinerCapabilities()
        capabilities.start()
        self.capabilities = capabilities
        app.setCapabilityCallback(sink: capabilities)
    }

    private static func configureStoragePath(for app: TwentyNinerApp) {
        guard let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first else {
            return
        }
        let directory = base.appendingPathComponent("NMP", isDirectory: true)
        do {
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
            if !app.setStoragePath(path: directory.path) {
                kbLog.fault("setStoragePath rejected — persistent storage NOT configured; init logic error")
                assertionFailure("setStoragePath rejected")
            }
        } catch {
            kbLog.error("failed to create NMP storage directory: \(error.localizedDescription, privacy: .public)")
        }
    }

    deinit {
        // Clear the update + capability sinks (quiescence: after each setter
        // returns no in-flight callback can still hold the sink), then tear down
        // the actor.
        app.setUpdateSink(sink: nil)
        updateSink = nil
        app.setCapabilityCallback(sink: nil)
        capabilities = nil
        app.shutdown()
    }

    /// Register the NMPU frame observer. `handler` runs on every snapshot frame;
    /// `onPanic` fires on the actor-death frame. Replacing the sink quiesces the
    /// prior one (UniFFI `set_update_sink` drain contract).
    func listen(
        _ handler: @escaping (KernelUpdateResult) -> Void,
        onPanic: @escaping () -> Void = {}
    ) {
        let sink = KernelUpdateSink(handler: handler, onPanic: onPanic)
        updateSink = sink
        app.setUpdateSink(sink: sink)
    }

    /// Actor-liveness probe (D7 pull-side, ADR-0028). `true` while the Rust
    /// actor thread is still running.
    func isAlive() -> Bool {
        app.isAlive()
    }

    func start(visibleLimit: UInt32 = 80, emitHz: UInt32 = 4) {
        app.start(visibleLimit: visibleLimit, emitHz: emitHz)
    }

    func configure(visibleLimit: UInt32, emitHz: UInt32) {
        app.configure(visibleLimit: visibleLimit, emitHz: emitHz)
    }

    func stop() {
        app.stop()
    }

    func reset() {
        app.reset()
    }

    // ── iOS scenePhase → kernel lifecycle bridge ─────────────────────────

    func lifecycleForeground() {
        app.lifecycleForeground()
    }

    func lifecycleBackground() {
        app.lifecycleBackground()
    }

    /// Add a relay to the kernel's relay set. `role` is an NMP relay role token.
    func addRelay(url: String, role: String) {
        app.addRelay(url: url, role: role)
    }

    /// Seed 29er's Rust-owned default relay set (D7). The kernel dedups against
    /// session-restored rows so re-seeding is a no-op. `true` when ≥1 relay was
    /// handed to the kernel.
    @discardableResult
    func seedDefaultRelays() -> Bool {
        app.seedDefaultRelays()
    }

    /// Seed relays from a `[["url","role"],…]` JSON array (the `NMP_TEST_RELAYS`
    /// override). `false` on malformed/empty so the caller falls back.
    func seedRelays(fromJSON json: String) -> Bool {
        app.seedRelaysFromJson(json: json)
    }

    @discardableResult
    func selectNip29Relay(_ relayUrl: String) -> Bool {
        app.relaySelectorSelectRelay(relayUrl: relayUrl)
    }

    @discardableResult
    func addNip29Relay(_ relayUrl: String) -> Bool {
        app.relaySelectorAddRelay(relayUrl: relayUrl)
    }

    @discardableResult
    func removeNip29Relay(_ relayUrl: String) -> Bool {
        app.relaySelectorRemoveRelay(relayUrl: relayUrl)
    }

    /// Sign in with a local nsec and activate it. D004: Swift hands the nsec to
    /// NMP once, never re-reads it.
    func signInNsec(_ nsec: String) {
        app.signinNsec(nsec: nsec, makeActive: true)
    }

    /// Remove an identity; the Rust actor owns the active-account transition.
    func removeAccount(_ pubkey: String) {
        app.removeAccount(identityId: pubkey)
    }

    func retryPublish(handle: String) {
        app.retryPublish(handle: handle)
    }

    func resolveProfileRef(pubkey: String, consumerID: String) {
        app.resolveProfileRef(key: pubkey, consumerId: consumerID)
    }

    func releaseProfileRef(pubkey: String, consumerID: String) {
        app.releaseProfileRef(key: pubkey, consumerId: consumerID)
    }
}

/// Generated `UpdateSink` implementation. Receives raw FlatBuffers
/// `nmp.transport.UpdateFrame` bytes on the Rust listener thread, decodes them,
/// and routes snapshot/panic to the handler. `@unchecked Sendable`: it only
/// forwards to closures that bounce onto the main runloop themselves.
final class KernelUpdateSink: UpdateSink, @unchecked Sendable {
    let handler: (KernelUpdateResult) -> Void
    /// D7 actor-death hook. Rust emits a FlatBuffers panic frame before the
    /// update channel closes; the host flips its fatal-error UI from here.
    let onPanic: () -> Void

    init(
        handler: @escaping (KernelUpdateResult) -> Void,
        onPanic: @escaping () -> Void
    ) {
        self.handler = handler
        self.onPanic = onPanic
    }

    func onUpdate(frame: Data) {
        guard let decoded = KernelHandle.decode(frame: frame) else { return }
        switch decoded {
        case let .snapshot(result):
            handler(result)
        case .panic:
            onPanic()
        }
    }
}

extension KernelHandle {
    /// Decode a FlatBuffers `nmp.transport.UpdateFrame` `Data` into the 29er
    /// `KernelDecodedUpdateFrame`. Returns `nil` on a decode error or a
    /// schema-version mismatch (the snapshot is dropped rather than misparsed).
    static func decode(frame data: Data) -> KernelDecodedUpdateFrame? {
        let start = ContinuousClock.now
        do {
            let frame = try KernelUpdateFrameDecoder.decode(data)
            switch frame {
            case let .snapshot(
                frameSchemaVersion, sessionId, snapshotEpoch, envelopes,
                rev, running, lastErrorToast, lastErrorCategory):
                guard frameSchemaVersion == KERNEL_SCHEMA_VERSION else {
                    kbLog.error("schema version mismatch: frame=\(frameSchemaVersion) host=\(KERNEL_SCHEMA_VERSION) — snapshot rejected")
                    return nil
                }
                // 29er S01 consumes only the discovered-groups + active-account
                // sidecars. New typed slots are added here as 29er grows
                // (mirroring Chirp's `KernelBridge+Decoding.swift`).
                let typedDiscoveredGroups = TypedDiscoveredGroupsDecoder.decode(from: envelopes)
                let typedGroupTree = TypedGroupTreeDecoder.decode(from: envelopes)
                let typedGroupChat = TypedGroupTimelineDecoder.decode(from: envelopes)
                let typedGroupRoster = TypedGroupRosterDecoder.decode(from: envelopes)
                let typedPublishOutbox = TypedPublishOutboxDecoder.decode(from: envelopes)
                let typedActiveAccount = TypedActiveAccountDecoder.decode(from: envelopes)
                let typedGroupDefaults = TypedGroupDefaultsDecoder.decode(from: envelopes)
                let typedRelaySelector = TypedRelaySelectorDecoder.decode(from: envelopes)
                let typedRelayDiagnostics = TypedRelayDiagnosticsDecoder.decode(from: envelopes)
                let duration = start.duration(to: .now)
                kbLog.info("decoded ok rev=\(rev) activeAccount=\(typedActiveAccount ?? "nil")")
                return .snapshot(
                    KernelUpdateResult(
                        typedDiscoveredGroups: typedDiscoveredGroups,
                        typedGroupTree: typedGroupTree,
                        typedGroupChat: typedGroupChat,
                        typedGroupRoster: typedGroupRoster,
                        typedPublishOutbox: typedPublishOutbox,
                        typedActiveAccount: typedActiveAccount,
                        typedGroupDefaults: typedGroupDefaults,
                        typedProjections: envelopes,
                        typedRelaySelector: typedRelaySelector,
                        typedRelayDiagnostics: typedRelayDiagnostics,
                        sessionId: sessionId,
                        snapshotEpoch: snapshotEpoch,
                        rev: rev,
                        running: running,
                        lastErrorToast: lastErrorToast,
                        lastErrorCategory: lastErrorCategory,
                        payloadBytes: data.count,
                        callbackReceivedAt: start,
                        decodeMicros: duration.microseconds
                    )
                )
            case let .panic(message):
                kbLog.fault("NMP_ACTOR_PANIC detected bytes=\(data.count) msg=\(message, privacy: .public)")
                return .panic(message)
            }
        } catch let error as DecodingError {
            switch error {
            case let .keyNotFound(key, ctx):
                kbLog.error("FlatBuffers decode: keyNotFound '\(key.stringValue)' at \(ctx.codingPath.map(\.stringValue).joined(separator: ".")) bytes=\(data.count)")
            case let .typeMismatch(_, ctx):
                kbLog.error("FlatBuffers decode: typeMismatch at \(ctx.codingPath.map(\.stringValue).joined(separator: ".")) — \(ctx.debugDescription) bytes=\(data.count)")
            default:
                kbLog.error("FlatBuffers decode error: \(error.localizedDescription) bytes=\(data.count)")
            }
            return nil
        } catch {
            kbLog.error("FlatBuffers snapshot decode error: \(error.localizedDescription) bytes=\(data.count)")
            return nil
        }
    }
}
