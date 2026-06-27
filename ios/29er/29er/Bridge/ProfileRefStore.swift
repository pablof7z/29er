import FlatBuffers
import Foundation

/// Host-side mirror of the kernel's `refs.profile` row-delta projection.
///
/// The kernel remains the source of truth. This store only merges typed
/// row-delta payloads pushed in update frames so SwiftUI registry components
/// can read one visible pubkey at render time.
@MainActor
final class ProfileRefStore {
    private struct Entry: Equatable {
        let rev: UInt64
        let payload: Data
    }

    private var rows: [String: Entry] = [:]
    private var appliedSession: UInt64 = 0
    private var appliedEpoch: UInt64 = 0

    func reset() {
        rows.removeAll()
        appliedSession = 0
        appliedEpoch = 0
    }

    func merge(payload: Data, sessionId: UInt64, snapshotEpoch: UInt64) -> Bool {
        guard !payload.isEmpty else { return false }
        var buffer = ByteBuffer(data: payload)
        let batch: nmp_refs_RefRowDeltaBatch
        do {
            batch = try getCheckedRoot(byteBuffer: &buffer, fileId: nmp_refs_RefRowDeltaBatch.id)
        } catch {
            return false
        }
        guard batch.namespace == "profile" else { return false }

        let identityChanged = sessionId != appliedSession || snapshotEpoch != appliedEpoch
        if batch.baseline {
            var next: [String: Entry] = [:]
            for row in batch.rows {
                guard let key = row.key else { return false }
                guard row.state != .cleared else { continue }
                let rowPayload = Data(row.payload)
                guard decodeProfile(payload: rowPayload, fallbackPubkey: key) != nil else {
                    return false
                }
                if let existing = next[key], row.rev <= existing.rev { continue }
                next[key] = Entry(rev: row.rev, payload: rowPayload)
            }
            let changed = rows != next || identityChanged
            rows = next
            appliedSession = sessionId
            appliedEpoch = snapshotEpoch
            return changed
        }

        guard !identityChanged else { return false }
        var changed = false
        for row in batch.rows {
            guard let key = row.key else { return false }
            if row.state == .cleared {
                if let existing = rows[key], row.rev > existing.rev {
                    rows.removeValue(forKey: key)
                    changed = true
                }
                continue
            }
            if let existing = rows[key], row.rev <= existing.rev { continue }
            let rowPayload = Data(row.payload)
            guard decodeProfile(payload: rowPayload, fallbackPubkey: key) != nil else {
                continue
            }
            rows[key] = Entry(rev: row.rev, payload: rowPayload)
            changed = true
        }
        return changed
    }

    func profile(forPubkey pubkey: String) -> ProfileWire? {
        guard let payload = rows[pubkey]?.payload else { return nil }
        return decodeProfile(payload: payload, fallbackPubkey: pubkey)
    }

    private func decodeProfile(payload: Data, fallbackPubkey: String) -> ProfileWire? {
        guard !payload.isEmpty else { return nil }
        var buffer = ByteBuffer(data: payload)
        let snapshot: nmp_kernel_ProfileSnapshot
        do {
            snapshot = try getCheckedRoot(byteBuffer: &buffer, fileId: nmp_kernel_ProfileSnapshot.id)
        } catch {
            return nil
        }
        guard let card = snapshot.card else { return nil }
        let pubkey = card.pubkey?.nonEmpty ?? fallbackPubkey
        let displayName = card.hasDisplayName ? card.displayName?.nonEmpty : card.name?.nonEmpty
        let pictureUrl = card.hasPictureUrl ? card.pictureUrl?.nonEmpty : nil
        return ProfileWire(
            pubkey: pubkey,
            displayName: displayName,
            about: card.about?.nonEmpty,
            pictureUrl: pictureUrl,
            nip05: card.nip05?.nonEmpty,
            npub: pubkey,
            npubShort: pubkey.shortHex
        )
    }
}

private extension String {
    var nonEmpty: String? { isEmpty ? nil : self }
}
