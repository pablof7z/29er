import Foundation
import os.log

let kbLog = Logger(subsystem: "io.f7z.app29er.bridge", category: "KernelBridge")

/// Mirror of `KERNEL_SCHEMA_VERSION` (Rust: `crates/nmp-core/src/update_envelope.rs`).
/// Must be bumped in lock-step when the Rust constant changes. A mismatch
/// causes `KernelHandle.decodeFlatBuffer` to reject the snapshot rather than
/// silently misparse renamed or retyped fields.
private let KERNEL_SCHEMA_VERSION: UInt32 = 1

/// Thin Swift wrapper around the generated `TwentyNinerApp` UniFFI facade
/// (`Bridge/Generated/nmp_app_29er.swift`). PR-4 of the C-ABI → UniFFI
/// migration: there is no more hand-marshaled `OpaquePointer` / `CString` /
/// `UnsafeMutableRawBufferPointer` traffic here — every call below is a plain
/// Swift method on the generated `TwentyNinerApp` object, which owns the
/// opaque Rust pointer internally.
///
/// 29er's minimal S01 surface: new/free (the generated class's own
/// init/deinit), update-sink registration, start/stop/reset, storage path,
/// liveness probe, identity (nsec sign-in), and relay bootstrap. The NIP-29
/// group-discovery + group-chat + dispatch helpers live on the `KernelHandle`
/// extension in `GroupDiscoveryBridge.swift` — that file still calls the
/// deleted C-ABI symbols (`nmp_app_29er_open_group_discovery` and friends)
/// and will not compile until PR-5 migrates it onto the facade once PR-2
/// lands the matching Rust methods. That is expected and out of scope here.
final class KernelHandle {
    /// The generated UniFFI facade object. Owns the opaque Rust pointer and
    /// its own lifecycle (`TwentyNinerApp.init`/`deinit` replace the old
    /// `nmp_app_new`/`nmp_app_free` pair).
    let app: TwentyNinerApp

    /// Retained update sink. UniFFI's callback-interface handle map keeps the
    /// Rust side's reference alive, but Swift must also hold a strong
    /// reference of its own so the object is not deallocated out from under
    /// the registered handle. Cleared in lock-step with `setUpdateSink(nil)`
    /// on re-`listen()` (replace) or in `deinit` (clear) — mirrors the old
    /// `Unmanaged.passRetained` discipline without the manual retain/release.
    private var retainedUpdateSink: KernelUpdateSink?
    /// Strong reference to the registered capabilities object. Held so the
    /// `CapabilitySink` adapter passed to `setCapabilityCallback` stays valid
    /// until `deinit` clears the callback.
    private var retainedCapabilities: TwentyNinerCapabilities?

    /// Last-applied snapshot revision. Mutated by `KernelModel.apply` on
    /// `@MainActor` (the apply path runs on the main actor). Read by the
    /// staleness guard. Not `@Published` — `rev` is not a view-facing value.
    /// Lives on the handle so extensions can read/write it without a stored
    /// property in an extension (illegal in Swift).
    var lastAppliedRev: UInt64 = 0

    init() {
        // "Construct + compose 29er. No IO; the actor is NOT started." — the
        // facade's `init()` now performs what used to be the separate
        // `nmp_app_29er_register` composition step (NIP-29 action
        // namespaces, group-create defaults projection, NIP-46 signer broker
        // init) internally. There is no longer a second opaque "29er
        // registration handle" to track.
        app = TwentyNinerApp()
        Self.configureStoragePath(for: app)
        // ADR-0053 — 29er is a full client: declare that it consumes every
        // kernel-owned built-in Tier-2 projection. Must run before
        // `app.start(...)`; the kernel narrows its built-in output to this
        // declaration (the one non-footgun way to receive the full set).
        app.declareConsumedProjections()
        // S02 — register the native keyring capability handler before any
        // `start()` so the kernel can route capability requests from the
        // first tick (the identity restore hook reads from Keychain during
        // actor startup). The handler is started immediately and held by
        // `retainedCapabilities` for the kernel lifetime.
        let capabilities = TwentyNinerCapabilities()
        capabilities.start()
        registerCapabilityHandler(capabilities)
    }

    private static var storageDirectory: URL? {
        guard let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first else {
            return nil
        }
        return base.appendingPathComponent("NMP", isDirectory: true)
    }

    private static func configureStoragePath(for app: TwentyNinerApp) {
        guard let directory = storageDirectory else { return }
        do {
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
            let ok = app.setStoragePath(path: directory.path)
            if !ok {
                kbLog.fault("setStoragePath returned false — persistent storage NOT configured; init logic error")
                assertionFailure("TwentyNinerApp.setStoragePath failed")
            }
        } catch {
            kbLog.error("failed to create NMP storage directory: \(error.localizedDescription, privacy: .public)")
        }
    }

    deinit {
        // Unregister the update sink and release the retained reference in
        // lock-step (balances the strong retain taken in `listen`).
        clearUpdateCallback()
        // Unregister the capability callback before releasing
        // `retainedCapabilities` so no callback fires with a dangling
        // reference.
        app.setCapabilityCallback(sink: nil)
        retainedCapabilities = nil
        app.shutdown()
    }

    /// Register the native keyring capability handler. The Rust kernel routes
    /// every keyring `CapabilityRequest` through this seam. Must be called
    /// before `start()` so the handler is in place for any capability requests
    /// the actor issues during startup (identity restore reads from Keychain).
    func registerCapabilityHandler(_ capabilities: TwentyNinerCapabilities) {
        retainedCapabilities = capabilities
        app.setCapabilityCallback(sink: capabilities)
    }

    /// Wire the Rust update sink. `handler` runs on every snapshot frame.
    /// Snapshot updates are binary-only FlatBuffers `nmp.transport.UpdateFrame`
    /// bytes delivered as `Data` through the generated `UpdateSink.onUpdate`
    /// callback interface. There is no runtime JSON fallback path.
    func listen(
        _ handler: @escaping (KernelUpdateResult) -> Void,
        onPanic: @escaping () -> Void = {}
    ) {
        // Clear any prior registration first. `setUpdateSink` quiesces (the
        // generated binding's doc comment: "After this returns, the previous
        // sink is neither registered nor mid-invocation") — after it returns
        // no in-flight callback can still hold the old sink, so releasing the
        // previous strong reference immediately afterwards is safe.
        clearUpdateCallback()
        let sink = KernelUpdateSink(handler: handler, onPanic: onPanic)
        retainedUpdateSink = sink
        app.setUpdateSink(sink: sink)
    }

    /// Unregister the Rust update sink and release the retained reference in
    /// lock-step. Idempotent.
    private func clearUpdateCallback() {
        guard retainedUpdateSink != nil else { return }
        app.setUpdateSink(sink: nil)
        retainedUpdateSink = nil
    }

    /// Actor-liveness probe (D7 pull-side, ADR-0028). Returns `true` when the
    /// Rust actor thread is still running, `false` when it has terminated
    /// (panic, clean Shutdown, or null app). Pairs with the panic envelope
    /// signal `listen(_:onPanic:)` subscribes to.
    func isAlive() -> Bool {
        app.isAlive()
    }

    func start(visibleLimit: UInt32 = 80, emitHz: UInt32 = 4) {
        app.start(visibleLimit: visibleLimit, emitHz: emitHz)
    }

    /// Reconfigure rendering limits without restarting (same clamps as
    /// `start`). Unlike the old minimal C-ABI header (which had no
    /// `nmp_app_configure` symbol and left this a no-op), the generated
    /// facade exposes it directly.
    func configure(visibleLimit: UInt32, emitHz: UInt32) {
        app.configure(visibleLimit: visibleLimit, emitHz: emitHz)
    }

    func stop() {
        app.stop()
    }

    func reset() {
        app.reset()
    }

    func resetLocalDatabase() throws {
        app.stop()
        if let directory = Self.storageDirectory {
            if FileManager.default.fileExists(atPath: directory.path) {
                try FileManager.default.removeItem(at: directory)
            }
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        }
        app.reset()
    }

    // ── T118 / G3 — iOS scenePhase → kernel lifecycle bridge ─────────────
    //
    // 29er's `@main` App observes `@Environment(\.scenePhase)` and routes
    // `.active` / `.background` through here. The kernel decides what each
    // phase MEANS (D7): scenePhase reports the fact, the kernel reacts.

    /// Report iOS scenePhase = `.active`. Idempotent.
    func lifecycleForeground() {
        app.lifecycleForeground()
    }

    /// Report iOS scenePhase = `.background`. Idempotent.
    func lifecycleBackground() {
        app.lifecycleBackground()
    }

    /// Add a relay to the kernel's relay set. `role` is a NMP relay role
    /// token (e.g. "outbox", "inbox"). Fire-and-forget (D6): a null app or
    /// invalid URL is a silent no-op.
    func addRelay(url: String, role: String) {
        app.addRelay(url: url, role: role)
    }

    func removeRelay(url: String) {
        app.removeRelay(url: url)
    }

    /// Seed 29er's Rust-owned default relay set (D7 — seeding policy lives in
    /// Rust, not the shell). Wraps `TwentyNinerApp.seedDefaultRelays`; the
    /// kernel dedups against session-restored rows so re-seeding is a no-op.
    /// Returns `true` when at least one relay was handed to the kernel.
    @discardableResult
    func seedDefaultRelays() -> Bool {
        app.seedDefaultRelays()
    }

    /// Seed relays from a `[["url","role"],…]` JSON array (the
    /// `NMP_TEST_RELAYS` override shape). Wraps
    /// `TwentyNinerApp.seedRelaysFromJson`; returns `false` on null/malformed/
    /// empty input so the caller falls back to `seedDefaultRelays()`. Parsing +
    /// validation live in Rust — Swift only forwards the env-var string.
    func seedRelays(fromJSON json: String) -> Bool {
        app.seedRelaysFromJson(json: json)
    }

    // TODO(follow-up PR): the NIP-29 relay-selector verbs
    // (`nmp_app_29er_relay_selector_{select,add,remove}_relay`) were part of
    // the deleted C-ABI's separate "29er registration handle" surface and
    // have no equivalent on the generated `TwentyNinerApp` facade yet — PR-1
    // only exposes the generic `dispatchAction` byte lane; the richer
    // per-namespace NIP-29 convenience is later work (alongside PR-2/PR-5).
    // Stubbed to a no-op `false` so `GroupTreeView`'s relay-selector UI
    // compiles and degrades safely instead of resurrecting a dead C symbol.
    @discardableResult
    func selectNip29Relay(_ relayUrl: String) -> Bool {
        false
    }

    @discardableResult
    func addNip29Relay(_ relayUrl: String) -> Bool {
        false
    }

    @discardableResult
    func removeNip29Relay(_ relayUrl: String) -> Bool {
        false
    }

    /// Sign in with a local nsec and activate it as the active account.
    /// Fire-and-forget (D6): the nsec is validated by `nostr::Keys::parse`
    /// in Rust. On success the `active_account` slot is populated and the
    /// `KACT` typed projection carries the pubkey on the next tick. On
    /// failure (malformed/invalid nsec) the slot stays nil — the shell
    /// observes the absence via `typedActiveAccount` and surfaces an error.
    ///
    /// D004: Swift hands the nsec to NMP once, never re-reads it. The caller
    /// must clear its own copy immediately after dispatch.
    func signInNsec(_ nsec: String) {
        app.signinNsec(nsec: nsec, makeActive: true)
    }

    /// Remove an identity. The Rust actor owns the resulting active-account
    /// transition and keyring forget work; Swift only names the current
    /// account to remove.
    func removeAccount(_ pubkey: String) {
        app.removeAccount(identityId: pubkey)
    }

    func retryPublish(handle: String) {
        app.retryPublish(handle: handle)
    }

    // TODO(follow-up PR): `nmp_app_resolve_profile_ref` /
    // `nmp_app_release_profile_ref` (ADR-0063 typed profile-ref adapters)
    // have no equivalent on the generated `TwentyNinerApp` facade yet either
    // — same gap as the NIP-29 relay selector above. Stubbed to a no-op
    // until a follow-up PR exposes the seam (most likely via the generic
    // `dispatchAction` byte lane once Rust defines a typed envelope for
    // `refs.profile`). This means avatar/profile-name resolution
    // (`NostrAvatar`, `NostrProfileHost`) does not claim new pubkeys until
    // that lands — flagged in the PR description, not silently dropped.
    func resolveProfileRef(pubkey: String, consumerID: String) {}

    func releaseProfileRef(pubkey: String, consumerID: String) {}
}

/// Adapts 29er's update handling to the generated `UpdateSink`
/// callback-interface protocol (replaces the old `NmpUpdateCallback` C
/// function pointer + `Unmanaged<KernelUpdateSink>` context dance).
///
/// `UpdateSink` requires `Sendable` (it crosses into Rust's callback handle
/// map); `@unchecked` because the stored closures are plain
/// `@MainActor`-hopping callbacks (mirroring `KernelModel.init()`'s existing
/// `DispatchQueue.main.async` + `MainActor.assumeIsolated` pattern below),
/// not because the type is internally mutated across threads.
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

    /// `UpdateSink.onUpdate` — called by Rust on every snapshot/panic frame
    /// with the raw FlatBuffers `nmp.transport.UpdateFrame` bytes, already
    /// copied into a `Data` value by the generated binding (no borrowed
    /// pointer to worry about, unlike the old C callback).
    func onUpdate(frame: Data) {
        guard let decoded = KernelHandle.decodeFlatBuffer(frame) else { return }
        switch decoded {
        case let .snapshot(result):
            handler(result)
        case .panic:
            onPanic()
        }
    }
}

/// Adapts 29er's keyring capability handler to the generated `CapabilitySink`
/// callback-interface protocol (replaces the old `NmpCapabilityCallback` C
/// function pointer + `strdup`/`nmp_free_string` contract). Rust invokes this
/// from the actor thread (never the main thread), so a synchronous capability
/// may block here safely.
///
/// `CapabilitySink` requires `Sendable`; this is a retroactive conformance
/// declared outside `TwentyNinerCapabilities`'s own file, so Swift requires
/// the `@unchecked` spelling here. `handleJSON` is already written to be
/// safely callable off the main thread (see `Capabilities/TwentyNinerCapabilities.swift`).
extension TwentyNinerCapabilities: CapabilitySink, @unchecked Sendable {
    func onCapabilityRequest(requestJson: String) -> String {
        handleJSON(requestJson)
    }
}

extension KernelHandle {
    /// Decode a FlatBuffers `nmp.transport.UpdateFrame` byte buffer into the
    /// 29er `KernelDecodedUpdateFrame`. Returns `nil` on a decode error or a
    /// schema-version mismatch (the snapshot is dropped rather than misparsed).
    static func decodeFlatBuffer(_ data: Data) -> KernelDecodedUpdateFrame? {
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
                let typedGroupChat = TypedGroupEventsDecoder.decode(from: envelopes)
                let typedGroupMembers = TypedGroupMembersDecoder.decode(from: envelopes)
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
                        typedGroupMembers: typedGroupMembers,
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
