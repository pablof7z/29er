import FlatBuffers
import Foundation

enum KernelUpdateFrameDecoderError: LocalizedError {
    case emptyPayload
    case missingSnapshotPayload
    case missingPanicPayload

    var errorDescription: String? {
        switch self {
        case .emptyPayload:
            return "empty FlatBuffers update payload"
        case .missingSnapshotPayload:
            return "snapshot frame missing payload"
        case .missingPanicPayload:
            return "panic frame missing payload"
        }
    }
}

enum KernelUpdateFrame {
    /// A decoded snapshot frame. `(schemaVersion, sessionId, snapshotEpoch,
    /// typedProjections, rev, running, lastErrorToast, lastErrorCategory)`.
    /// 29er reads the bare envelope scalars off the `SnapshotFrame` table
    /// directly (no Tier-3 `TypedSnapshotEnvelope` struct in S01 — that lands
    /// when 29er grows a metrics/diagnostics surface).
    case snapshot(
        UInt32,
        UInt64,
        UInt64,
        [TypedProjectionEnvelope],
        UInt64,
        Bool,
        String?,
        String?)
    case panic(String)
}

/// The wire presence state of one typed projection row, mirroring the
/// `nmp_transport_ProjectionPresenceState` FlatBuffers enum. `Unchanged` is
/// never on the wire — absence IS Unchanged per ADR-0055 D3.
enum WireProjectionState: UInt8 {
    case changed = 0
    case cleared = 1
}

/// ADR-0037: a typed FlatBuffers sidecar. Each envelope wraps one named
/// projection's opaque NFTS/NFCT bytes plus its schema identity. Hosts that
/// recognise a `schemaId` decode the bytes with the matching typed decoder;
/// others ignore it.
struct TypedProjectionEnvelope {
    let key: String
    let schemaId: String
    let schemaVersion: UInt32
    let fileIdentifier: String
    let payload: Data
    let projectionRev: UInt64
    let state: WireProjectionState

    init(
        key: String,
        schemaId: String,
        schemaVersion: UInt32,
        fileIdentifier: String,
        payload: Data,
        projectionRev: UInt64 = 1,
        state: WireProjectionState = .changed
    ) {
        self.key = key
        self.schemaId = schemaId
        self.schemaVersion = schemaVersion
        self.fileIdentifier = fileIdentifier
        self.payload = payload
        self.projectionRev = projectionRev
        self.state = state
    }
}

enum KernelUpdateFrameDecoder {
    static func decode(_ data: Data) throws -> KernelUpdateFrame {
        guard !data.isEmpty else { throw KernelUpdateFrameDecoderError.emptyPayload }
        var buffer = ByteBuffer(data: data)
        // Buffers cross a trusted in-process FFI boundary (Rust kernel → Swift
        // shell, same process, same memory). Using the unchecked getRoot
        // accessor matches Chirp's posture; the fileId/magic is not checked
        // here but the TypedProjectionEnvelope key+schemaId routing already
        // selects the right sub-buffer.
        let frame: nmp_transport_UpdateFrame = getRoot(byteBuffer: &buffer)

        switch frame.kind {
        case .snapshot:
            guard let snapshot = frame.snapshot else {
                throw KernelUpdateFrameDecoderError.missingSnapshotPayload
            }
            let envelopes = extractTypedProjections(from: snapshot)
            return .snapshot(
                snapshot.schemaVersion,
                snapshot.sessionId,
                snapshot.snapshotEpoch,
                envelopes,
                snapshot.rev,
                snapshot.running,
                snapshot.lastErrorToast,
                snapshot.lastErrorCategory)
        case .panic:
            guard let message = frame.panic?.msg else {
                throw KernelUpdateFrameDecoderError.missingPanicPayload
            }
            return .panic(message)
        }
    }

    /// ADR-0037: lift the typed projection sidecar into plain Swift envelopes.
    /// Projections missing a key are skipped so a malformed entry never aborts
    /// the whole snapshot.
    private static func extractTypedProjections(
        from snapshot: nmp_transport_SnapshotFrame
    ) -> [TypedProjectionEnvelope] {
        var envelopes: [TypedProjectionEnvelope] = []
        let projections = snapshot.typedProjections
        envelopes.reserveCapacity(projections.count)
        for projection in projections {
            guard let key = projection.key else { continue }
            let state: WireProjectionState = projection.state == .cleared ? .cleared : .changed
            let projectionRev = projection.projectionRev
            let (schemaId, schemaVersion, fileIdentifier, payload): (String, UInt32, String, Data)
            if let typed = projection.payload, let sid = typed.schemaId {
                (schemaId, schemaVersion, fileIdentifier, payload) = (
                    sid,
                    typed.schemaVersion,
                    typed.fileIdentifier ?? "",
                    Data(typed.payload)
                )
            } else if state == .cleared {
                (schemaId, schemaVersion, fileIdentifier, payload) = ("", 0, "", Data())
            } else {
                continue
            }
            envelopes.append(TypedProjectionEnvelope(
                key: key,
                schemaId: schemaId,
                schemaVersion: schemaVersion,
                fileIdentifier: fileIdentifier,
                payload: payload,
                projectionRev: projectionRev,
                state: state
            ))
        }
        return envelopes
    }
}