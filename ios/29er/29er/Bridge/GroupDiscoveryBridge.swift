import Foundation
import os.log

// ─────────────────────────────────────────────────────────────────────────
// NIP-29 group-discovery + join FFI bridge.
//
// Sibling of Chirp's `GroupDiscoveryBridge.swift` — the read + write sides of
// the NIP-29 discover / join screen, mirroring the same `KernelHandle`
// extension + `@MainActor ObservableObject` store pattern. 29er's S01
// surface needs only the read side (open/close discovery + the discover
// action); the join action is wired for parity with Chirp and for T06's
// shakeout view.
//
// Thin-shell rule (29er): ZERO protocol logic in Swift. The Rust
// `DiscoveredGroupsProjection` owns kind:39000/39001/39002 filtering,
// replaceable-event merging, and alphabetical ordering; the
// `nmp.nip29.discover` action owns the relay-pinned `LogicalInterest`; the
// `nmp.nip29.join` action owns the kind:9021 event + tags + signing. Swift
// only marshals JSON across the FFI and mirrors the snapshot.

private let gdLog = Logger(subsystem: "io.f7z.app29er.bridge", category: "GroupDiscoveryBridge")

// ── KernelHandle NIP-29 discovery + join extension (C-FFI surface) ────────

extension KernelHandle {
    /// Wire a NIP-29 `GroupChatProjection` for `groupId` into the kernel. Pure
    /// consumption: messages surface under `nmp.nip29.group_chat` on snapshots.
    func registerGroupChat(groupId: GroupId) {
        guard
            let data = try? JSONSerialization.data(withJSONObject: groupId.jsonObject),
            let json = String(data: data, encoding: .utf8)
        else {
            gdLog.error("registerGroupChat: failed to encode GroupId JSON")
            return
        }
        json.withCString { nmp_app_29er_register_group_chat(raw, $0) }
        gdLog.info("registered NIP-29 group chat projection for \(groupId.localId, privacy: .public)")
    }

    /// Open a NIP-29 group-discovery session for `hostRelayUrl`.
    ///
    /// Returns an opaque handle the caller MUST pass to
    /// `closeGroupDiscovery(_:)` when the session ends (screen dismissed or
    /// relay switched). Returns `nil` when the relay URL is empty or
    /// registration fails (D6).
    func openGroupDiscovery(hostRelayUrl: String) -> OpaquePointer? {
        guard !hostRelayUrl.isEmpty else { return nil }
        let ptr = hostRelayUrl.withCString {
            nmp_app_29er_open_group_discovery(raw, $0)
        }
        guard let ptr else { return nil }
        gdLog.info("opened NIP-29 discovery session for \(hostRelayUrl, privacy: .public)")
        return OpaquePointer(ptr)
    }

    /// Close a group-discovery session previously opened with
    /// `openGroupDiscovery(hostRelayUrl:)`.
    ///
    /// Unregisters the observer and removes the snapshot projection so no
    /// stale group catalog is emitted after the session ends. The `handle`
    /// MUST NOT be used after this call. A nil handle is a no-op.
    func closeGroupDiscovery(_ handle: OpaquePointer?) {
        guard let handle else { return }
        nmp_app_29er_close_group_discovery(UnsafeMutableRawPointer(handle))
        gdLog.info("closed NIP-29 discovery session")
    }

    /// Mark one group's direct messages read inside the Rust group-tree
    /// projection. The tree projection owns unread aggregation; Swift only
    /// reports the user's current read position.
    func markGroupRead(_ handle: OpaquePointer?, groupId: String) {
        guard let handle, !groupId.isEmpty else { return }
        groupId.withCString {
            nmp_app_29er_mark_group_read(UnsafeMutableRawPointer(handle), $0)
        }
    }

    /// Select the group whose member rows should be emitted by the Rust
    /// `nmp.nip29.group_members` projection.
    func selectGroupMembers(_ handle: OpaquePointer?, groupId: String) {
        guard let handle, !groupId.isEmpty else { return }
        groupId.withCString {
            nmp_app_29er_select_group_members(UnsafeMutableRawPointer(handle), $0)
        }
    }

    /// Dispatch a `nmp.nip29.discover` action — push the relay-pinned
    /// `LogicalInterest` for kinds 39000/39001/39002 so the kernel opens a
    /// REQ for that relay's group catalog. Fire-and-forget; the catalog
    /// surfaces through the next `nmp.nip29.discovered_groups` snapshot tick.
    func discoverGroups(relayUrl: String) {
        let payload: [String: Any] = ["relay_url": relayUrl]
        _ = dispatchNip29("nmp.nip29.discover", payload: payload, label: "discoverGroups")
    }

    /// Dispatch a `nmp.nip29.join` action — publish a kind:9021 join request
    /// to `group`'s host relay. Fire-and-forget; the relay's response (a new
    /// kind:39002 listing the user) surfaces through the next discovery
    /// snapshot tick.
    func joinGroup(group: GroupId, inviteCode: String? = nil, reason: String? = nil) {
        var payload: [String: Any] = ["group": group.jsonObject]
        if let inviteCode, !inviteCode.isEmpty {
            payload["invite_code"] = inviteCode
        }
        if let reason, !reason.isEmpty {
            payload["reason"] = reason
        }
        _ = dispatchNip29("nmp.nip29.join", payload: payload, label: "joinGroup")
    }

    /// Dispatch a `nmp.nip29.post_chat_message` action — publish a kind:9
    /// message to `group`. Rust owns the event shape, tags, signing, and
    /// relay pinning; Swift only marshals the draft text.
    func postChatMessage(group: GroupId, content: String, mentionPubkeys: [String] = []) -> Bool {
        var payload: [String: Any] = [
            "group": group.jsonObject,
            "content": content,
        ]
        if !mentionPubkeys.isEmpty {
            payload["mention_pubkeys"] = mentionPubkeys
        }
        return dispatchNip29(
            "nmp.nip29.post_chat_message",
            payload: payload,
            label: "postChatMessage"
        )
    }

    /// Dispatch a `nmp.nip29.react_in_group` action — publish a host-pinned
    /// kind:7 reaction to a message in `group`.
    func reactToMessage(
        group: GroupId,
        eventId: String,
        eventAuthorPubkey: String? = nil,
        reaction: String = "+"
    ) -> Bool {
        var payload: [String: Any] = [
            "group": group.jsonObject,
            "target_event_id": eventId,
            "content": reaction,
        ]
        if let eventAuthorPubkey, !eventAuthorPubkey.isEmpty {
            payload["target_author_pubkey"] = eventAuthorPubkey
        }
        return dispatchNip29("nmp.nip29.react_in_group", payload: payload, label: "reactToMessage")
    }

    /// Shared marshal for NIP-29 action dispatches. Encodes `payload` to JSON
    /// and routes it through the 29er byte doorway
    /// `nmp_app_29er_dispatch_action_bytes`; returns true only when Rust
    /// accepts the typed action envelope and returns a correlation id.
    /// Snapshot/outbox state still owns eventual delivery.
    private func dispatchNip29(
        _ namespace: String, payload: [String: Any], label: String
    ) -> Bool {
        guard
            let data = try? JSONSerialization.data(withJSONObject: payload),
            let json = String(data: data, encoding: .utf8)
        else {
            gdLog.error("\(label, privacy: .public): failed to encode action payload")
            return false
        }
        return json.withCString { jsonPtr in
            namespace.withCString { nsPtr in
                guard let ptr = nmp_app_29er_dispatch_action_bytes(raw, nsPtr, jsonPtr) else {
                    gdLog.error("\(label, privacy: .public): action dispatch returned null")
                    return false
                }
                defer { nmp_free_string(ptr) }

                let rawResult = String(cString: ptr)
                guard let resultData = rawResult.data(using: .utf8),
                      let result = try? JSONDecoder().decode(Nip29DispatchResult.self, from: resultData)
                else {
                    gdLog.error("\(label, privacy: .public): malformed dispatch result \(rawResult, privacy: .public)")
                    return false
                }

                if let error = result.error, !error.isEmpty {
                    gdLog.error("\(label, privacy: .public): dispatch rejected: \(error, privacy: .public)")
                    return false
                }
                guard let correlationId = result.correlationId, !correlationId.isEmpty else {
                    gdLog.error("\(label, privacy: .public): dispatch result had no correlation id")
                    return false
                }
                return true
            }
        }
    }
}

private struct Nip29DispatchResult: Decodable {
    let correlationId: String?
    let error: String?

    private enum CodingKeys: String, CodingKey {
        case correlationId = "correlation_id"
        case error
    }
}

@MainActor
extension KernelModel {
    func sendGroupMessage(groupId: String, content: String, mentionPubkeys: [String] = []) -> Bool {
        let trimmed = content.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty, let group = nip29GroupId(for: groupId) else { return false }
        return kernel.postChatMessage(
            group: group,
            content: trimmed,
            mentionPubkeys: mentionPubkeys
        )
    }

    func reactToGroupMessage(groupId: String, eventId: String, eventAuthorPubkey: String) {
        guard let group = nip29GroupId(for: groupId) else { return }
        _ = kernel.reactToMessage(
            group: group,
            eventId: eventId,
            eventAuthorPubkey: eventAuthorPubkey
        )
    }

    func nip29GroupId(for groupId: String) -> GroupId? {
        guard let node = groupTree.allNodes[groupId] else { return nil }
        return GroupId(hostRelayUrl: node.hostRelayUrl, localId: node.groupId)
    }
}

// ── DiscoveredGroupsStore — projection mirror pushed by KernelModel.apply ─

/// `@MainActor` store backing the discover screen. A pure mirror of the
/// kernel's `nip29.discovered_groups` projection plus the discover / join
/// dispatchers — no Swift owns any group state, ordering, or protocol
/// decision (thin-shell rule).
///
/// Lifecycle is handle-keyed: on the first search against a relay
/// `openGroupDiscovery` is called to register the read projection and a
/// handle is stored here. On relay switch the old handle is closed before
/// opening a new one so there is never a bounded observer leak.
@MainActor
final class DiscoveredGroupsStore: ObservableObject {
    /// The relay this store is currently scoped to. Empty until the user
    /// enters one and taps Search. `groups` is `[]` while empty.
    @Published private(set) var hostRelayUrl: String = ""

    /// Alphabetically-ordered discovered groups, mirrored verbatim from the
    /// kernel projection. Ordering is owned by the Rust
    /// `DiscoveredGroupsProjection`.
    @Published private(set) var groups: [DiscoveredGroup] = []

    /// `true` between a discover dispatch and the first non-empty snapshot
    /// tick. Drives a "Searching…" indicator on the view. Cleared once any
    /// snapshot arrives (empty or not).
    @Published private(set) var isSearching: Bool = false

    private unowned let kernel: KernelHandle

    /// The opaque Rust handle for the currently-open discovery session.
    /// `nil` until the user first searches. Closed on relay switch or deinit.
    ///
    /// Always mutate via `setDiscoveryHandle(_:)` — it keeps `_discoveryHandleRaw`
    /// in sync so the nonisolated `deinit` can close the handle safely.
    private var discoveryHandle: OpaquePointer?

    /// Nonisolated mirror of `discoveryHandle`. Updated in lock-step by
    /// `setDiscoveryHandle(_:)`. Only ever read from `deinit`, which runs
    /// after the last reference is released — no concurrent MainActor
    /// mutation can occur at that point, making the unsafety sound.
    nonisolated(unsafe) private var _discoveryHandleRaw: OpaquePointer?

    init(kernel: KernelHandle) {
        self.kernel = kernel
    }

    deinit {
        // Swift 6: `deinit` is nonisolated and cannot touch `@MainActor`-isolated
        // state. `_discoveryHandleRaw` mirrors `discoveryHandle` exactly.
        // `nmp_app_29er_close_group_discovery` is a plain C function — it takes
        // no Swift state and needs no actor. By the time `deinit` runs there
        // are no remaining references, so no concurrent mutation of
        // `_discoveryHandleRaw` is possible: the unsafety is sound.
        if let raw = _discoveryHandleRaw {
            nmp_app_29er_close_group_discovery(UnsafeMutableRawPointer(raw))
        }
    }

    /// Update both handle fields atomically. Always runs on the MainActor.
    private func setDiscoveryHandle(_ handle: OpaquePointer?) {
        discoveryHandle = handle
        _discoveryHandleRaw = handle
    }

    /// Begin a discover session against `relayUrl`: open the read projection
    /// for this relay (closing any prior session) and dispatch
    /// `nmp.nip29.discover`. Whitespace / empty input is dropped here (the
    /// Rust validator also rejects empty/non-wss input).
    func searchGroups(relayUrl: String) {
        let trimmed = relayUrl.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }

        if trimmed != hostRelayUrl {
            kernel.closeGroupDiscovery(discoveryHandle)
            setDiscoveryHandle(nil)
            groups = []
        }
        hostRelayUrl = trimmed

        if discoveryHandle == nil {
            setDiscoveryHandle(kernel.openGroupDiscovery(hostRelayUrl: trimmed))
        }
        isSearching = true
        kernel.discoverGroups(relayUrl: trimmed)
    }

    /// Close the current discovery session (if any). Tears down the read
    /// projection so no stale group catalog is emitted after the discover
    /// screen is dismissed.
    func closeSession() {
        kernel.closeGroupDiscovery(discoveryHandle)
        setDiscoveryHandle(nil)
        groups = []
        hostRelayUrl = ""
        isSearching = false
    }

    func markGroupRead(groupId: String) {
        kernel.markGroupRead(discoveryHandle, groupId: groupId)
    }

    func selectGroupMembers(groupId: String) {
        kernel.selectGroupMembers(discoveryHandle, groupId: groupId)
    }

    /// Mirror the latest kernel snapshot. Called from `KernelModel.apply`
    /// on every tick. A snapshot whose `hostRelayUrl` does not match the
    /// store's current target is ignored (we may receive one stale tick
    /// while the user is mid-switch). Empty `groups` is honoured — the relay
    /// may genuinely host none.
    func apply(snapshot: DiscoveredGroupsSnapshot?) {
        guard let snapshot else { return }
        guard snapshot.hostRelayUrl == hostRelayUrl else { return }
        if snapshot.groups != groups {
            groups = snapshot.groups
        }
        if isSearching {
            isSearching = false
        }
    }
}
