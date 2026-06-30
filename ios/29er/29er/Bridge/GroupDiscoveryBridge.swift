import Foundation
import os.log

private let gdLog = Logger(subsystem: "io.f7z.app29er.bridge", category: "GroupDiscoveryBridge")

extension KernelHandle {
    func registerGroupChat(groupId: GroupId) {
        guard
            let data = try? JSONSerialization.data(withJSONObject: groupId.jsonObject),
            let json = String(data: data, encoding: .utf8)
        else {
            gdLog.error("registerGroupChat: failed to encode GroupId JSON")
            return
        }
        _ = app.openGroupChat(groupIdJson: json)
    }

    @discardableResult
    func openGroupDiscovery(hostRelayUrl: String) -> Bool {
        guard !hostRelayUrl.isEmpty else { return false }
        return app.openGroupDiscovery(hostRelayUrl: hostRelayUrl)
    }

    func closeGroupDiscovery() {
        app.closeGroupDiscovery()
    }

    @discardableResult
    func refreshGroupDiscovery(hostRelayUrl: String) -> Bool {
        app.refreshGroupDiscovery(hostRelayUrl: hostRelayUrl)
    }

    func markGroupRead(groupId: String) {
        guard !groupId.isEmpty else { return }
        app.markGroupRead(localId: groupId)
    }

    func openGroupRoster(groupId: GroupId) {
        guard
            let data = try? JSONSerialization.data(withJSONObject: groupId.jsonObject),
            let json = String(data: data, encoding: .utf8)
        else {
            gdLog.error("openGroupRoster: failed to encode GroupId JSON")
            return
        }
        _ = app.openGroupRoster(groupIdJson: json)
    }

    func closeGroupRoster() {
        app.closeGroupRoster()
    }

    func discoverGroups(relayUrl: String) {
        let payload: [String: Any] = ["relay_url": relayUrl]
        _ = dispatchNip29("nmp.nip29.discover", payload: payload, label: "discoverGroups")
    }

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

    func leaveGroup(group: GroupId, reason: String? = nil) -> Bool {
        var payload: [String: Any] = ["group": group.jsonObject]
        if let reason, !reason.isEmpty {
            payload["reason"] = reason
        }
        return dispatchNip29("nmp.nip29.leave", payload: payload, label: "leaveGroup")
    }

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

    func createInvite(group: GroupId, codes: [String]) -> Bool {
        let payload: [String: Any] = [
            "group": group.jsonObject,
            "codes": codes,
        ]
        return dispatchNip29("nmp.nip29.create_invite", payload: payload, label: "createInvite")
    }

    func setParent(group: GroupId, parent: String?) -> Bool {
        var payload: [String: Any] = ["group": group.jsonObject]
        if let parent, !parent.isEmpty {
            payload["parent"] = parent
        }
        return dispatchNip29("nmp.nip29.set_parent", payload: payload, label: "setParent")
    }

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

    func reactToMessage(
        group: GroupId,
        eventId: String,
        eventAuthorPubkey: String? = nil,
        reaction: String = "+"
    ) -> Bool {
        guard
            let data = try? JSONSerialization.data(withJSONObject: group.jsonObject),
            let json = String(data: data, encoding: .utf8)
        else {
            gdLog.error("reactToMessage: failed to encode GroupId JSON")
            return false
        }
        let outcome = app.reactToGroupMessage(
            groupIdJson: json,
            eventId: eventId,
            eventAuthorPubkey: eventAuthorPubkey,
            reaction: reaction
        )
        if let error = outcome.error, !error.isEmpty {
            gdLog.error("reactToMessage: dispatch rejected: \(error, privacy: .public)")
            return false
        }
        return outcome.correlationId?.isEmpty == false
    }

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
        return outcome.correlationId?.isEmpty == false
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
        let activeRelayUrl = relaySelector.activeRelayUrl
        let hostRelayUrl = activeRelayUrl.isEmpty ? groupDefaults.suggestedRelayUrl : activeRelayUrl
        let group = GroupId(hostRelayUrl: hostRelayUrl, localId: localId)
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

@MainActor
final class DiscoveredGroupsStore: ObservableObject {
    @Published private(set) var hostRelayUrl: String = ""
    @Published private(set) var groups: [DiscoveredGroup] = []
    @Published private(set) var isSearching: Bool = false

    private unowned let kernel: KernelHandle
    private var openSessionRelay: String?

    init(kernel: KernelHandle) {
        self.kernel = kernel
    }

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

    func closeSession() {
        if openSessionRelay != nil {
            kernel.closeGroupDiscovery()
            openSessionRelay = nil
        }
        groups = []
        hostRelayUrl = ""
        isSearching = false
    }

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
