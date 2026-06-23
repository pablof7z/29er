import FlatBuffers
import Foundation

/// Hand-written typed-projection decoders for 29er's S01 surface. Mirrors the
/// shape of Chirp's generated `TypedProjectionDecoders.generated.swift` but
/// holds only the two sidecars 29er consumes: `active_account` (`KACT`) and
/// `nmp.nip29.discovered_groups` (`NDGS`). When 29er grows to consume more
/// kernel projections, copy the matching decoder from Chirp's generated file
/// (or regenerate via `cargo run -p nmp-codegen -- gen swift …`).
enum TypedActiveAccountDecoder {
    /// `TypedProjection.key` the producer publishes for this projection.
    static let key = "active_account"
    /// `TypedPayload.schema_id` carried on the sidecar buffer.
    static let schemaId = "active_account"
    /// FlatBuffers `file_identifier` for `nmp_kernel_ActiveAccountSnapshot`.
    static let fileIdentifier = "KACT"

    /// Decode the typed `active_account` sidecar from the snapshot's
    /// typed-projection envelopes into the 29er domain value. Returns `nil`
    /// when the sidecar is absent, carries the wrong schema, or is not a
    /// well-formed buffer.
    static func decode(from projections: [TypedProjectionEnvelope]) -> String? {
        guard let projection = projections.first(where: {
            $0.key == key && $0.schemaId == schemaId
        }), !projection.payload.isEmpty else {
            return nil
        }
        return decode(bytes: projection.payload)
    }

    /// Decode a raw `KACT` FlatBuffers buffer into the 29er domain value.
    static func decode(bytes: Data) -> String? {
        guard !bytes.isEmpty else { return nil }
        var buffer = ByteBuffer(data: bytes)
        let reader: nmp_kernel_ActiveAccountSnapshot = getRoot(byteBuffer: &buffer)
        return TypedProjectionGlue.activeAccount(reader)
    }
}

enum TypedDiscoveredGroupsDecoder {
    /// `TypedProjection.key` the producer publishes for this projection.
    static let key = "nmp.nip29.discovered_groups"
    /// `TypedPayload.schema_id` carried on the sidecar buffer.
    static let schemaId = "nmp.nip29.discovered_groups"
    /// FlatBuffers `file_identifier` for `nmp_nip29_DiscoveredGroupsSnapshot`.
    static let fileIdentifier = "NDGS"

    /// Decode the typed `nmp.nip29.discovered_groups` sidecar from the
    /// snapshot's typed-projection envelopes into the 29er domain value.
    /// Returns `nil` when the sidecar is absent, carries the wrong schema, or
    /// is not a well-formed buffer.
    static func decode(from projections: [TypedProjectionEnvelope]) -> DiscoveredGroupsSnapshot? {
        guard let projection = projections.first(where: {
            $0.key == key && $0.schemaId == schemaId
        }), !projection.payload.isEmpty else {
            return nil
        }
        return decode(bytes: projection.payload)
    }

    /// Decode a raw `NDGS` FlatBuffers buffer into the 29er domain value.
    static func decode(bytes: Data) -> DiscoveredGroupsSnapshot? {
        guard !bytes.isEmpty else { return nil }
        var buffer = ByteBuffer(data: bytes)
        let reader: nmp_nip29_DiscoveredGroupsSnapshot = getRoot(byteBuffer: &buffer)
        return TypedProjectionGlue.discoveredGroups(reader)
    }
}