import Foundation

/// Capability injection point for 29er.
///
/// The kernel grants the app a set of capability *sockets*; the app supplies
/// the platform implementation. This holder is the one place those
/// implementations are constructed and started, mirroring the thin-bridge
/// pattern in `Bridge/KernelBridge.swift`.
///
/// It owns the `KeychainCapability` (at-rest secret storage). Rust decides
/// when and what to call; Swift only executes the request and reports the raw
/// result (D7).
///
/// There is a single capability sink (the generated `CapabilitySink`, wired via
/// `TwentyNinerApp.setCapabilityCallback`); it routes by the `namespace` field
/// of the incoming `CapabilityRequest` — see [`handleJSON(_:)`].
///
/// `@unchecked Sendable`: the Rust actor calls `onCapabilityRequest` from its
/// worker thread (never the main thread). The owned `KeychainCapability` is
/// internally serialized, so a synchronous capability may block here safely.
final class TwentyNinerCapabilities: CapabilitySink, @unchecked Sendable {
    let keyring: KeychainCapability

    init(
        keyring: KeychainCapability = KeychainCapability()
    ) {
        self.keyring = keyring
    }

    /// Idempotent: start all owned capabilities. Safe to call on every app
    /// foreground.
    func start() {
        keyring.start()
    }

    /// Idempotent: mark capabilities inactive. Does not erase stored secrets.
    func stop() {
        keyring.stop()
    }

    /// `CapabilitySink` conformance — the kernel calls this (on its worker
    /// thread) with a `CapabilityRequest` JSON and expects a
    /// `CapabilityEnvelope` JSON back. Delegates to [`handleJSON(_:)`].
    func onCapabilityRequest(requestJson: String) -> String {
        handleJSON(requestJson)
    }

    /// Single capability-callback entry point. Routes the raw kernel
    /// `CapabilityRequest` JSON to the capability owning its `namespace` and
    /// returns the raw `CapabilityEnvelope` JSON.
    ///
    /// D6: an unparseable request or an unknown namespace yields a populated
    /// error envelope string, never a thrown error and never `nil`.
    func handleJSON(_ requestJSON: String) -> String {
        guard
            let data = requestJSON.data(using: .utf8),
            let request = try? JSONDecoder().decode(CapabilityRequest.self, from: data)
        else {
            // Cannot even read the namespace — return a generic error envelope.
            let env = CapabilityEnvelope(
                namespace: "",
                correlationID: "",
                resultJSON: "{\"status\":\"error\",\"message\":\"malformed-request\"}")
            return Self.encode(env) ?? "{}"
        }

        switch request.namespace {
        case KeychainCapability.namespace:
            return keyring.handleJSON(requestJSON)
        default:
            // D6 — an unknown namespace is data, not a crash. Echo the
            // correlation id so the issuing kernel module can still correlate.
            let env = CapabilityEnvelope(
                namespace: request.namespace,
                correlationID: request.correlationID,
                resultJSON: "{\"status\":\"error\",\"message\":\"unknown-namespace\"}")
            return Self.encode(env) ?? "{}"
        }
    }

    private static func encode<T: Encodable>(_ value: T) -> String? {
        guard let data = try? JSONEncoder().encode(value) else { return nil }
        return String(data: data, encoding: .utf8)
    }
}