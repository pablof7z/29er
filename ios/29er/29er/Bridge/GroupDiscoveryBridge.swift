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
    /// Wire a NIP-29 group's chat-message read view (kinds 9 + 11) into the
    /// kernel. Pure consumption: messages surface under
    /// `nmp.nip29.group_timeline` on snapshots. Singleton in Rust — re-opening
    /// replaces the prior group-events view.
    func registerGroupChat(groupId: GroupId) {
        guard
            let data = try? JSONSerialization.data(withJSONObject: groupId.jsonObject),
            let json = String(data: data, encoding: .utf8)
        else {
            gdLog.error("registerGroupChat: failed to encode GroupId JSON")
            return
        }
        app.registerGroupChat(groupIdJson: json)
        gdLog.info("registered NIP-29 group timeline projection for \(groupId.localId, privacy: .public)")
    }

    /// Open a NIP-29 group-discovery session for `hostRelayUrl`. The Rust object
    /// owns the singleton session (re-opening closes the prior one first), so no
    /// handle crosses the boundary. Returns `true` on success, `false` when the
    /// relay URL is empty or the session failed to open (D6).
    @discardableResult
    func openGroupDiscovery(hostRelayUrl: String) -> Bool {
        guard !hostRelayUrl.isEmpty else { return false }
        let opened = app.openGroupDiscovery(hostRelayUrl: hostRelayUrl)
        if opened {
            gdLog.info("opened NIP-29 discovery session for \(hostRelayUrl, privacy: .public)")
        }
        return opened
    }

    /// Close the current group-discovery session (if any). Unregisters the
    /// observer and removes the snapshot projection so no stale group catalog is
    /// emitted after the session ends. Idempotent.
    func closeGroupDiscovery() {
        app.closeGroupDiscovery()
        gdLog.info("closed NIP-29 discovery session")
    }

    /// Refresh group discovery after a local store reset: tear down the current
    /// session, open a fresh one, and re-dispatch `nmp.nip29.discover`. `true`
    /// when the new session opened.
    @discardableResult
    func refreshGroupDiscovery(hostRelayUrl: String) -> Bool {
        app.refreshGroupDiscovery(hostRelayUrl: hostRelayUrl)
    }

    /// Mark one group's direct messages read inside the Rust group-tree
    /// projection. The tree projection owns unread aggregation; Swift only
    /// reports the user's current read position. No-op when no discovery
    /// session is open.
    func markGroupRead(groupId: String) {
        guard !groupId.isEmpty else { return }
        app.markGroupRead(groupId: groupId)
    }

    /// Open the NIP-29 member-roster read view for `group`. The Rust
    /// `TwentyNinerApp` owns the singleton roster session (re-opening for a new
    /// group replaces the prior view), and the canonical
    /// `open_nip29_group_roster_session` door owns the relay-pinned
    /// 39001/39002/39003 interest + the `nmp.nip29.group_roster` (`NGRS`) typed
    /// sidecar — so no handle crosses the boundary. The roster surfaces on the
    /// next snapshot tick under `typedGroupRoster`.
    func openGroupRoster(groupId: GroupId) {
        guard
            let data = try? JSONSerialization.data(withJSONObject: groupId.jsonObject),
            let json = String(data: data, encoding: .utf8)
        else {
            gdLog.error("openGroupRoster: failed to encode GroupId JSON")
            return
        }
        _ = app.openGroupRoster(groupIdJson: json)
        gdLog.info("opened NIP-29 member roster for \(groupId.localId, privacy: .public)")
    }

    /// Close the current NIP-29 member-roster view (if any). Reclaims the
    /// `nmp.nip29.group_roster` sidecar + relay-pinned interest. Idempotent.
    func closeGroupRoster() {
        app.closeGroupRoster()
        gdLog.info("closed NIP-29 member roster")
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
    /// to `group`'s host relay. The relay response surfaces later as a new
    /// kind:39002 member-list snapshot.
    func joinGroup(group: GroupId, inviteCode: String? = nil, reason: String? = nil) -> Bool {
        var payload: [String: Any] = ["group": group.jsonObject]
        if let inviteCode, !inviteCode.isEmpty {
            payload["invite_code"] = inviteCode
        }
        if let reason, !reason.isEmpty {
            payload["reason"] = reason
        }
        return dispatchNip29("nmp.nip29.join", payload: payload, label: "joinGroup")
    }

    /// Dispatch a `nmp.nip29.leave` action — publish a kind:9022 leave request
    /// to `group`'s host relay. Rust owns event shape, tags, and host pinning.
    func leaveGroup(group: GroupId, reason: String? = nil) -> Bool {
        var payload: [String: Any] = ["group": group.jsonObject]
        if let reason, !reason.isEmpty {
            payload["reason"] = reason
        }
        return dispatchNip29("nmp.nip29.leave", payload: payload, label: "leaveGroup")
    }

    /// Dispatch a `nmp.nip29.create_public_group` action. The Rust action
    /// publishes kind:9007 followed by kind:9002 metadata.
    func createPublicGroup(
        group: GroupId,
        name: String,
        about: String? = nil,
        picture: String? = nil,
        visibility: String = "public",
        access: String = "open",
        parent: String? = nil
    ) -> Bool {
        var payload: [String: Any] = [
            "group": group.jsonObject,
            "name": name,
            "visibility": visibility,
            "access": access,
        ]
        if let about, !about.isEmpty {
            payload["about"] = about
        }
        if let picture, !picture.isEmpty {
            payload["picture"] = picture
        }
        if let parent, !parent.isEmpty {
            payload["parent"] = parent
        }
        return dispatchNip29(
            "nmp.nip29.create_public_group",
            payload: payload,
            label: "createPublicGroup"
        )
    }

    /// Dispatch a `nmp.nip29.put_user` action — add/promote a user by pubkey.
    func putUser(
        group: GroupId,
        targetPubkey: String,
        role: String? = nil,
        reason: String? = nil
    ) -> Bool {
        var payload: [String: Any] = [
            "group": group.jsonObject,
            "target_pubkey": targetPubkey,
        ]
        if let role, !role.isEmpty {
            payload["role"] = role
        }
        if let reason, !reason.isEmpty {
            payload["reason"] = reason
        }
        return dispatchNip29("nmp.nip29.put_user", payload: payload, label: "putUser")
    }

    /// Dispatch a `nmp.nip29.create_invite` action — mint invite codes.
    func createInvite(group: GroupId, codes: [String]) -> Bool {
        let payload: [String: Any] = [
            "group": group.jsonObject,
            "codes": codes,
        ]
        return dispatchNip29("nmp.nip29.create_invite", payload: payload, label: "createInvite")
    }

    /// Dispatch a `nmp.nip29.set_parent` action. Omit `parent` to detach to
    /// root; provide a group local id to adopt under that parent.
    func setParent(group: GroupId, parent: String?) -> Bool {
        var payload: [String: Any] = ["group": group.jsonObject]
        if let parent, !parent.isEmpty {
            payload["parent"] = parent
        }
        return dispatchNip29("nmp.nip29.set_parent", payload: payload, label: "setParent")
    }

    /// Dispatch the `nmp.nip29.post_chat_message` chat-send doorway — the
    /// stable app-level entrypoint (shared with the TUI) that takes raw text
    /// (carrying `@<pubkey>` placeholders) plus the `@mentioned` pubkeys. As of
    /// nmp-nip29 v0.8.0 the `nmp-app-29er` FFI runs the shared Rust composer
    /// (`compose_chat_message`: NIP-21 `@<hex>` → `nostr:npub1…` rewrite +
    /// deduplicated `["p", …]` tags) server-side, wraps the result as a kind:9
    /// `PublishGroupEventInput`, and re-emits it under the real
    /// `nmp.nip29.publish_group_event` action. Swift hand-formats nothing: it
    /// only marshals the draft text + mention pubkeys. Keep the namespace as
    /// the chat-send doorway key — dispatching `publish_group_event` directly
    /// would bypass composition (that action expects `{group, kind, content,
    /// tags}`, not raw text) and silently drop mentions.
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
    /// and routes it through `TwentyNinerApp.dispatchNip29Action`, which builds
    /// the typed payload + envelope in Rust and returns a typed
    /// `DispatchOutcome`. Returns true only when Rust accepts the action and
    /// mints a correlation id. Snapshot/outbox state still owns eventual
    /// delivery.
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
        let outcome = app.dispatchNip29Action(namespace: namespace, bodyJson: json)
        if let error = outcome.error, !error.isEmpty {
            gdLog.error("\(label, privacy: .public): dispatch rejected: \(error, privacy: .public)")
            return false
        }
        guard let correlationId = outcome.correlationId, !correlationId.isEmpty else {
            gdLog.error("\(label, privacy: .public): dispatch result had no correlation id")
            return false
        }
        return true
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

    func joinGroup(groupId: String, inviteCode: String? = nil, reason: String? = nil) -> Bool {
        guard let group = nip29GroupId(for: groupId) else { return false }
        return kernel.joinGroup(group: group, inviteCode: inviteCode, reason: reason)
    }

    func leaveGroup(groupId: String, reason: String? = nil) -> Bool {
        guard let group = nip29GroupId(for: groupId) else { return false }
        return kernel.leaveGroup(group: group, reason: reason)
    }

    func createGroup(
        localId: String,
        name: String,
        about: String? = nil,
        picture: String? = nil,
        visibility: String = "public",
        access: String = "open",
        parent: String? = nil
    ) -> Bool {
        // Host relay comes from a Rust-owned projection (D7), never a Swift
        // literal. A new room is created on the relay the user is currently
        // browsing — the active relay-selector projection (`activeRelayUrl`),
        // which follows the seeded relay set (so the `NMP_TEST_RELAYS` seam
        // flows through to create-group, letting it target local croissant in
        // tests). It falls back to the operator-policy suggested host relay
        // (`group_defaults.suggestedRelayUrl`) only when no relay is selected
        // yet. The Rust create action validates the host relay, so an empty URL
        // is rejected there rather than silently defaulted here.
        let activeRelayUrl = relaySelector.activeRelayUrl
        let hostRelayUrl = activeRelayUrl.isEmpty ? groupDefaults.suggestedRelayUrl : activeRelayUrl
        let group = GroupId(
            hostRelayUrl: hostRelayUrl,
            localId: localId
        )
        return kernel.createPublicGroup(
            group: group,
            name: name,
            about: about,
            picture: picture,
            visibility: visibility,
            access: access,
            parent: parent
        )
    }

    func putUser(
        groupId: String,
        targetPubkey: String,
        role: String? = nil,
        reason: String? = nil
    ) -> Bool {
        guard let group = nip29GroupId(for: groupId) else { return false }
        return kernel.putUser(
            group: group,
            targetPubkey: targetPubkey,
            role: role,
            reason: reason
        )
    }

    func createInvite(groupId: String, codes: [String]) -> Bool {
        guard let group = nip29GroupId(for: groupId) else { return false }
        return kernel.createInvite(group: group, codes: codes)
    }

    func setParent(groupId: String, parent: String?) -> Bool {
        guard let group = nip29GroupId(for: groupId) else { return false }
        return kernel.setParent(group: group, parent: parent)
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

    /// The relay of the currently-open Rust discovery session, or `nil` when no
    /// session is open. The Rust `TwentyNinerApp` owns the singleton session
    /// (and reclaims it on `shutdown`), so Swift holds no opaque handle — this
    /// is only a redundant-open guard.
    private var openSessionRelay: String?

    init(kernel: KernelHandle) {
        self.kernel = kernel
    }

    /// Begin a discover session against `relayUrl`: open the read projection
    /// for this relay (closing any prior session) and dispatch
    /// `nmp.nip29.discover`. Whitespace / empty input is dropped here (the
    /// Rust validator also rejects empty/non-wss input).
    func searchGroups(relayUrl: String) {
        let trimmed = relayUrl.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }

        if trimmed != hostRelayUrl {
            if openSessionRelay != nil {
                kernel.closeGroupDiscovery()
                openSessionRelay = nil
            }
            groups = []
        }
        hostRelayUrl = trimmed

        if openSessionRelay != trimmed, kernel.openGroupDiscovery(hostRelayUrl: trimmed) {
            openSessionRelay = trimmed
        }
        isSearching = true
        kernel.discoverGroups(relayUrl: trimmed)
    }

    /// Close the current discovery session (if any). Tears down the read
    /// projection so no stale group catalog is emitted after the discover
    /// screen is dismissed.
    func closeSession() {
        if openSessionRelay != nil {
            kernel.closeGroupDiscovery()
            openSessionRelay = nil
        }
        groups = []
        hostRelayUrl = ""
        isSearching = false
    }

    /// Reopen discovery after a local database reset: re-dispatch discover
    /// against `relayUrl` through the Rust object's `refresh_group_discovery`
    /// (which reopens the session + re-issues `nmp.nip29.discover`).
    func refreshSessionAfterLocalDatabaseReset(relayUrl: String) {
        let trimmed = relayUrl.trimmingCharacters(in: .whitespacesAndNewlines)
        groups = []
        guard !trimmed.isEmpty else {
            hostRelayUrl = ""
            openSessionRelay = nil
            isSearching = false
            return
        }
        hostRelayUrl = trimmed
        isSearching = true
        openSessionRelay = kernel.refreshGroupDiscovery(hostRelayUrl: trimmed) ? trimmed : nil
    }

    func markGroupRead(groupId: String) {
        kernel.markGroupRead(groupId: groupId)
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
