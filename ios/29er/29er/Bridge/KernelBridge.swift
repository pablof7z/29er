import Darwin
import Foundation
import os.log

let kbLog = Logger(subsystem: "io.f7z.app29er.bridge", category: "KernelBridge")

/// Mirror of `KERNEL_SCHEMA_VERSION` (Rust: `crates/nmp-core/src/update_envelope.rs`).
/// Must be bumped in lock-step when the Rust constant changes. A mismatch
/// causes `KernelBridge.decode()` to reject the snapshot rather than silently
/// misparse renamed or retyped fields.
private let KERNEL_SCHEMA_VERSION: UInt32 = 1

/// Thin C-FFI wrapper around the `nmp_core` static library.
///
/// 29er's minimal S01 surface: new/free, update callback registration, start
/// / stop / reset, storage path, liveness probe, identity (nsec sign-in), and
/// relay bootstrap. The NIP-29 group-discovery + group-chat + dispatch helpers
/// live on the `KernelHandle` extension in `GroupDiscoveryBridge.swift`.
final class KernelHandle {
    let raw: UnsafeMutableRawPointer
    /// Retained handle for the update sink whose opaque pointer is registered
    /// with Rust via `nmp_app_set_update_callback`. We `passRetained` the sink
    /// into Rust (Rust owns the +1) and hold the `Unmanaged` token here so the
    /// retain can be released *exactly once* — on re-`listen()` (replace) or
    /// in `deinit` (clear).
    private var retainedUpdateSink: Unmanaged<KernelUpdateSink>?
    /// Strong reference to the registered capabilities object. Held so the
    /// context pointer passed to `nmpCapabilityCallback` stays valid until
    /// `deinit` unregisters the callback.
    private var retainedCapabilities: TwentyNinerCapabilities?
    /// Opaque handle returned by `nmp_app_29er_register`. The
    /// group-discovery bridge extension manages its lifetime; see
    /// `GroupDiscoveryBridge.swift`.
    var app29erHandle: UnsafeMutableRawPointer?

    /// Last-applied snapshot revision. Mutated by `KernelModel.apply` on
    /// `@MainActor` (the apply path runs on the main actor). Read by the
    /// staleness guard. Not `@Published` — `rev` is not a view-facing value.
    /// Lives on the handle so extensions can read/write it without a stored
    /// property in an extension (illegal in Swift).
    var lastAppliedRev: UInt64 = 0

    init() {
        raw = nmp_app_new()
        Self.configureStoragePath(for: raw)
        // Stage 4 of NIP-46 wiring: initialise the bunker broker before any
        // `signInBunker(...)` dispatch can reach the actor. The broker
        // registers a hook with `nmp-core` that drives the NIP-46 connect /
        // get_public_key handshake on a worker thread. 29er does not use
        // bunker sign-in in S01, but the broker is part of the canonical NMP
        // composition and must be initialised before `nmp_app_start`.
        let brokerResult = nmp_signer_broker_init(raw)
        if brokerResult != 0 {
            kbLog.fault("nmp_signer_broker_init returned \(brokerResult) — bunker broker NOT active; init logic error")
            assertionFailure("nmp_signer_broker_init failed with code \(brokerResult)")
        }
        // 29er composition: register the canonical NMP defaults + the NIP-29
        // action namespaces + the group-create defaults projection. The
        // returned handle is held for `nmp_app_29er_unregister` in `deinit`.
        var handle: UnsafeMutableRawPointer?
        let registerStatus = nmp_app_29er_register(raw, &handle)
        if registerStatus != NmpRegisterStatus_Ok.rawValue {
            kbLog.fault("nmp_app_29er_register returned \(registerStatus) — 29er composition NOT registered; init logic error")
            assertionFailure("nmp_app_29er_register failed with code \(registerStatus)")
        }
        app29erHandle = handle
        // ADR-0053 — 29er is a full client: declare that it consumes every
        // kernel-owned built-in Tier-2 projection. Must run before
        // `nmp_app_start`; the kernel narrows its built-in output to this
        // declaration (the one non-footgun way to receive the full set).
        nmp_app_29er_declare_consumed_projections(raw)
        // S02 — register the native keyring capability handler before any
        // `nmp_app_start` so the kernel can route capability requests from
        // the first tick (the identity restore hook reads from Keychain
        // during `register_defaults` → `nmp_app_start`). The handler is
        // started immediately and held by `retainedCapabilities` for the
        // kernel lifetime.
        let capabilities = TwentyNinerCapabilities()
        capabilities.start()
        registerCapabilityHandler(capabilities)
    }

    private static func configureStoragePath(for raw: UnsafeMutableRawPointer) {
        guard let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first else {
            return
        }
        let directory = base.appendingPathComponent("NMP", isDirectory: true)
        do {
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
            let status = directory.path.withCString { nmp_app_set_storage_path(raw, $0) }
            if status != 0 {
                kbLog.fault("nmp_app_set_storage_path returned \(status) — persistent storage NOT configured; init logic error")
                assertionFailure("nmp_app_set_storage_path failed with code \(status)")
            }
        } catch {
            kbLog.error("failed to create NMP storage directory: \(error.localizedDescription, privacy: .public)")
        }
    }

    deinit {
        // T146 / Chirp parity — drop the 29er registration BEFORE
        // `nmp_app_free` per FFI contract.
        if let handle = app29erHandle {
            nmp_app_29er_unregister(handle)
            app29erHandle = nil
        }
        // Unregister the update callback and release the retained sink in
        // lock-step (balances the `passRetained` in `listen`).
        clearUpdateCallback()
        // Unregister the capability callback before releasing
        // `retainedCapabilities` so no callback fires with a dangling
        // context pointer.
        nmp_app_set_capability_callback(raw, nil, nil)
        retainedCapabilities = nil
        nmp_app_free(raw)
    }

    /// Register the native keyring capability handler. The Rust kernel routes
    /// every keyring `CapabilityRequest` through this seam. Must be called
    /// before `start()` so the handler is in place for any capability requests
    /// the actor issues during startup (identity restore reads from Keychain).
    func registerCapabilityHandler(_ capabilities: TwentyNinerCapabilities) {
        retainedCapabilities = capabilities
        nmp_app_set_capability_callback(
            raw,
            Unmanaged.passUnretained(capabilities).toOpaque(),
            nmpCapabilityCallback)
    }

    /// Wire the Rust update callback. `handler` runs on every snapshot frame.
    /// Snapshot updates are binary-only FlatBuffers `nmp.transport.UpdateFrame`
    /// bytes. There is no runtime JSON fallback path.
    func listen(
        _ handler: @escaping (KernelUpdateResult) -> Void,
        onPanic: @escaping () -> Void = {}
    ) {
        // Clear any prior registration first. `set_update_callback` quiesces
        // (Article: UpdateCallbackGate) — after it returns no in-flight
        // callback can still hold the old context pointer — so releasing the
        // previous retain immediately afterwards is safe.
        clearUpdateCallback()
        let sink = KernelUpdateSink(handler: handler, onPanic: onPanic)
        // `passRetained` hands Rust its own +1 on the sink; the matching
        // release happens in `clearUpdateCallback()` (on replace or deinit).
        let retained = Unmanaged.passRetained(sink)
        retainedUpdateSink = retained
        nmp_app_set_update_callback(
            raw,
            retained.toOpaque(),
            nmpUpdateCallback)
    }

    /// Unregister the Rust update callback and release the sink retain in
    /// lock-step. Idempotent. Relies on the `nmp_app_set_update_callback`
    /// quiescence guarantee: once the setter returns, the actor has drained any
    /// in-flight callback, so no Rust caller can dereference the (about to be
    /// released) context pointer.
    private func clearUpdateCallback() {
        guard let retained = retainedUpdateSink else { return }
        nmp_app_set_update_callback(raw, nil, nil)
        retained.release()
        retainedUpdateSink = nil
    }

    /// Actor-liveness probe (D7 pull-side, ADR-0028). Returns `true` when the
    /// Rust actor thread is still running, `false` when it has terminated
    /// (panic, clean Shutdown, or null app). Pairs with the panic envelope
    /// signal `listen(_:onPanic:)` subscribes to.
    func isAlive() -> Bool {
        nmp_app_is_alive(raw) == 1
    }

    func start(visibleLimit: UInt32 = 80, emitHz: UInt32 = 4) {
        nmp_app_start(raw, visibleLimit, emitHz)
    }

    func configure(visibleLimit: UInt32, emitHz: UInt32) {
        // `nmp_app_configure` is not in 29er's minimal header; added when 29er
        // grows a settings surface. Left as a no-op for S01 parity with Chirp.
        _ = visibleLimit
        _ = emitHz
    }

    func stop() {
        nmp_app_stop(raw)
    }

    func reset() {
        nmp_app_reset(raw)
    }

    // ── T118 / G3 — iOS scenePhase → kernel lifecycle bridge ─────────────
    //
    // 29er's `@main` App observes `@Environment(\.scenePhase)` and routes
    // `.active` / `.background` through here. The kernel decides what each
    // phase MEANS (D7): scenePhase reports the fact, the kernel reacts.

    /// Report iOS scenePhase = `.active`. Idempotent.
    func lifecycleForeground() {
        nmp_app_lifecycle_foreground(raw)
    }

    /// Report iOS scenePhase = `.background`. Idempotent.
    func lifecycleBackground() {
        nmp_app_lifecycle_background(raw)
    }

    /// Add a relay to the kernel's relay set. `role` is a NMP relay role
    /// token (e.g. "outbox", "inbox"). Fire-and-forget (D6): a null app or
    /// invalid URL is a silent no-op.
    func addRelay(url: String, role: String) {
        url.withCString { urlPtr in
            role.withCString { rolePtr in
                nmp_app_add_relay(raw, urlPtr, rolePtr)
            }
        }
    }

    /// Seed 29er's Rust-owned default relay set (D7 — seeding policy lives in
    /// Rust, not the shell). Wraps `nmp_app_29er_seed_default_relays`; the
    /// kernel dedups against session-restored rows so re-seeding is a no-op.
    /// Returns `true` when at least one relay was handed to the kernel.
    @discardableResult
    func seedDefaultRelays() -> Bool {
        nmp_app_29er_seed_default_relays(raw)
    }

    /// Seed relays from a `[["url","role"],…]` JSON array (the
    /// `NMP_TEST_RELAYS` override shape). Wraps
    /// `nmp_app_29er_seed_relays_from_json`; returns `false` on null/malformed/
    /// empty input so the caller falls back to `seedDefaultRelays()`. Parsing +
    /// validation live in Rust — Swift only forwards the env-var string.
    func seedRelays(fromJSON json: String) -> Bool {
        json.withCString { nmp_app_29er_seed_relays_from_json(raw, $0) }
    }

    @discardableResult
    func selectNip29Relay(_ relayUrl: String) -> Bool {
        guard let app29erHandle else { return false }
        return relayUrl.withCString {
            nmp_app_29er_relay_selector_select_relay(app29erHandle, $0)
        }
    }

    @discardableResult
    func addNip29Relay(_ relayUrl: String) -> Bool {
        guard let app29erHandle else { return false }
        return relayUrl.withCString {
            nmp_app_29er_relay_selector_add_relay(app29erHandle, $0)
        }
    }

    @discardableResult
    func removeNip29Relay(_ relayUrl: String) -> Bool {
        guard let app29erHandle else { return false }
        return relayUrl.withCString {
            nmp_app_29er_relay_selector_remove_relay(app29erHandle, $0)
        }
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
        nsec.withCString { nmp_app_signin_nsec(raw, $0, 1) }
    }

    /// Remove an identity. The Rust actor owns the resulting active-account
    /// transition and keyring forget work; Swift only names the current
    /// account to remove.
    func removeAccount(_ pubkey: String) {
        pubkey.withCString { nmp_app_remove_account(raw, $0) }
    }

    func retryPublish(handle: String) {
        handle.withCString { nmp_app_retry_publish(raw, $0) }
    }

    func resolveProfileRef(pubkey: String, consumerID: String) {
        pubkey.withCString { pubkeyPtr in
            consumerID.withCString { consumerPtr in
                nmp_app_resolve_profile_ref(raw, pubkeyPtr, consumerPtr)
            }
        }
    }

    func releaseProfileRef(pubkey: String, consumerID: String) {
        pubkey.withCString { pubkeyPtr in
            consumerID.withCString { consumerPtr in
                nmp_app_release_profile_ref(raw, pubkeyPtr, consumerPtr)
            }
        }
    }
}

final class KernelUpdateSink {
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
}

let nmpUpdateCallback: NmpUpdateCallback = { context, bytes, count in
    guard let context, let bytes, count > 0 else { return }
    let sink = Unmanaged<KernelUpdateSink>.fromOpaque(context).takeUnretainedValue()
    guard let frame = KernelHandle.decodeFlatBuffer(
        bytes: UnsafeRawPointer(bytes),
        count: Int(count)
    ) else {
        return
    }
    switch frame {
    case let .snapshot(result):
        sink.handler(result)
    case .panic:
        sink.onPanic()
    }
}

/// C capability callback — receives `CapabilityRequest` JSON from Rust and
/// returns a malloc-allocated `CapabilityEnvelope` JSON string that Rust frees
/// via `nmp_free_string` / `CString::from_raw`. Uses `strdup` so the
/// allocation is compatible with Rust's `CString::from_raw` on Apple platforms
/// (both use the system malloc allocator).
///
/// There is one C callback for every capability; `TwentyNinerCapabilities.handleJSON`
/// routes the request to the capability owning its `namespace` (keyring). Rust
/// invokes this from the actor thread (never the main thread), so a synchronous
/// capability may block here safely.
let nmpCapabilityCallback: NmpCapabilityCallback = { context, requestJSON in
    guard let context, let requestJSON else { return nil }
    let capabilities = Unmanaged<TwentyNinerCapabilities>.fromOpaque(context).takeUnretainedValue()
    let requestStr = String(cString: requestJSON)
    let resultStr = capabilities.handleJSON(requestStr)
    return resultStr.withCString { strdup($0) }
}

extension KernelHandle {
    /// Decode a FlatBuffers `nmp.transport.UpdateFrame` byte buffer into the
    /// 29er `KernelDecodedUpdateFrame`. Returns `nil` on a decode error or a
    /// schema-version mismatch (the snapshot is dropped rather than misparsed).
    static func decodeFlatBuffer(
        bytes: UnsafeRawPointer,
        count: Int
    ) -> KernelDecodedUpdateFrame? {
        let start = ContinuousClock.now
        let data = Data(bytes: bytes, count: count)
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
                        sessionId: sessionId,
                        snapshotEpoch: snapshotEpoch,
                        typedRelaySelector: typedRelaySelector,
                        typedRelayDiagnostics: typedRelayDiagnostics,
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
