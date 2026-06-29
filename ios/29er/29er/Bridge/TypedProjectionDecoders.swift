import FlatBuffers
import Foundation

/// Hand-written typed-projection decoders for 29er's current Swift surface.
/// Mirrors the shape of Chirp's generated
/// `TypedProjectionDecoders.generated.swift` but only includes the sidecars
/// 29er consumes. When 29er grows to consume more kernel projections, copy the
/// matching decoder from Chirp's generated file or regenerate this surface.
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

enum TypedGroupTimelineDecoder {
    // The canonical NMP group-events read session (`nmp-native-runtime`
    // `open_nip29_group_events_session`) registers its typed sidecar under the
    // key/schema `nmp.nip29.group_events` with the `NGEV` file identifier — NOT
    // the pre-migration `nmp.nip29.group_timeline` / `NGTL` of the deleted
    // hand-written C-ABI. The shell must decode the key the runtime actually
    // emits, or the group timeline silently stays empty. The `NGEV`
    // `GroupEventsSnapshot` wire layout is byte-identical to the `NGTL`
    // `GroupTimelineSnapshot` (same vtable offsets: id/pubkey/content/createdAt/
    // kind, schema_version + events), so the generated `NGTL` reader decodes the
    // `NGEV` payload unchanged — only the routing key/schema strings differ.
    static let key = "nmp.nip29.group_events"
    static let schemaId = "nmp.nip29.group_events"
    static let fileIdentifier = "NGEV"

    static func decode(from projections: [TypedProjectionEnvelope]) -> GroupChatSnapshot? {
        guard let projection = projections.first(where: {
            $0.key == key && $0.schemaId == schemaId
        }), !projection.payload.isEmpty else {
            return nil
        }
        return decode(bytes: projection.payload)
    }

    static func decode(bytes: Data) -> GroupChatSnapshot? {
        guard !bytes.isEmpty else { return nil }
        var buffer = ByteBuffer(data: bytes)
        let reader: nmp_nip29_GroupTimelineSnapshot = getRoot(byteBuffer: &buffer)
        return TypedProjectionGlue.groupTimeline(reader)
    }
}

enum TypedGroupMembersDecoder {
    static let key = "nmp.nip29.group_members"
    static let schemaId = "nmp.nip29.group_members"
    static let fileIdentifier = "NGMS"

    static func decode(from projections: [TypedProjectionEnvelope]) -> GroupMembersSnapshot? {
        guard let projection = projections.first(where: {
            $0.key == key && $0.schemaId == schemaId
        }), !projection.payload.isEmpty else {
            return nil
        }
        return decode(bytes: projection.payload)
    }

    static func decode(bytes: Data) -> GroupMembersSnapshot? {
        guard !bytes.isEmpty else { return nil }
        var buffer = ByteBuffer(data: bytes)
        let reader: nmp_nip29_GroupMembersSnapshot = getRoot(byteBuffer: &buffer)
        return TypedProjectionGlue.groupMembers(reader)
    }
}

enum TypedPublishOutboxDecoder {
    static let key = "publish_outbox"
    static let schemaId = "publish_outbox"
    static let fileIdentifier = "KPBO"

    static func decode(from projections: [TypedProjectionEnvelope]) -> [PublishOutboxItem]? {
        guard let projection = projections.first(where: {
            $0.key == key && $0.schemaId == schemaId
        }), !projection.payload.isEmpty else {
            return nil
        }
        return decode(bytes: projection.payload)
    }

    static func decode(bytes: Data) -> [PublishOutboxItem]? {
        guard !bytes.isEmpty else { return nil }
        var buffer = ByteBuffer(data: bytes)
        let reader: nmp_kernel_PublishOutboxSnapshot = getRoot(byteBuffer: &buffer)
        return TypedProjectionGlue.publishOutbox(reader)
    }
}

enum TypedGroupDefaultsDecoder {
    static let key = "nmp.nip29.group_defaults"
    static let schemaId = "nmp.nip29.group_defaults"
    static let fileIdentifier = "NGDF"

    static func decode(from projections: [TypedProjectionEnvelope]) -> GroupDefaultsSnapshot? {
        guard let projection = projections.first(where: {
            $0.key == key && $0.schemaId == schemaId
        }), !projection.payload.isEmpty else {
            return nil
        }
        return decode(bytes: projection.payload)
    }

    static func decode(bytes: Data) -> GroupDefaultsSnapshot? {
        guard !bytes.isEmpty else { return nil }
        var buffer = ByteBuffer(data: bytes)
        let reader: nmp_nip29_GroupDefaultsSnapshot = getRoot(byteBuffer: &buffer)
        return TypedProjectionGlue.groupDefaults(reader)
    }
}

enum TypedGroupTreeDecoder {
    static let key = "nmp.29er.group_tree"
    static let schemaId = "nmp.29er.group_tree"
    static let fileIdentifier = "N29T"

    static func decode(from projections: [TypedProjectionEnvelope]) -> GroupTreeSnapshot? {
        guard let projection = projections.first(where: {
            $0.key == key && $0.schemaId == schemaId
        }), !projection.payload.isEmpty else {
            return nil
        }
        return decode(bytes: projection.payload)
    }

    static func decode(bytes: Data) -> GroupTreeSnapshot? {
        guard !bytes.isEmpty else { return nil }
        var buffer = ByteBuffer(data: bytes)
        let reader: nmp_app_29er_GroupTreeSnapshot = getRoot(byteBuffer: &buffer)
        return TypedProjectionGlue.groupTree(reader)
    }
}

enum TypedRelaySelectorDecoder {
    static let key = "nmp.29er.relay_selector"
    static let schemaId = "nmp.29er.relay_selector"
    static let fileIdentifier = "N29R"

    static func decode(from projections: [TypedProjectionEnvelope]) -> RelaySelectorSnapshot? {
        guard let projection = projections.first(where: {
            $0.key == key && $0.schemaId == schemaId
        }), !projection.payload.isEmpty else {
            return nil
        }
        return decode(bytes: projection.payload)
    }

    static func decode(bytes: Data) -> RelaySelectorSnapshot? {
        guard !bytes.isEmpty else { return nil }
        var buffer = ByteBuffer(data: bytes)
        let reader: nmp_app_29er_RelaySelectorSnapshot = getRoot(byteBuffer: &buffer)
        return TypedProjectionGlue.relaySelector(reader)
    }
}

enum TypedRelayDiagnosticsDecoder {
    static let key = "relay_diagnostics"
    static let schemaId = "relay_diagnostics"
    static let fileIdentifier = "KRDG"

    static func decode(from projections: [TypedProjectionEnvelope]) -> RelayDiagnosticsSnapshot? {
        guard let projection = projections.first(where: {
            $0.key == key && $0.schemaId == schemaId
        }), !projection.payload.isEmpty else {
            return nil
        }
        return decode(bytes: projection.payload)
    }

    static func decode(bytes: Data) -> RelayDiagnosticsSnapshot? {
        guard !bytes.isEmpty else { return nil }
        var buffer = ByteBuffer(data: bytes)
        let reader: nmp_kernel_RelayDiagnosticsSnapshot = getRoot(byteBuffer: &buffer)
        return TypedProjectionGlue.relayDiagnostics(reader)
    }
}
