import Foundation

struct RawGroup {
    let id: String
    let name: String
    let parent: String?
}

enum RelayMessage {
    case event([String: Any])
    case notice(String)
    case eose(String)
}

actor RelayService {
    private let relayURL: URL
    private let session: URLSession

    init(relayURL: URL = URL(string: "wss://nip29.f7z.io")!) {
        self.relayURL = relayURL
        self.session = URLSession.shared
    }

    private func openSubscription(_ filter: [String: Any]) async throws -> (URLSessionWebSocketTask, String) {
        let task = session.webSocketTask(with: relayURL)
        task.resume()
        let subId = "sub_\(UUID().uuidString.prefix(8))"
        let req: [Any] = ["REQ", subId, filter]
        let data = try JSONSerialization.data(withJSONObject: req)
        let text = String(data: data, encoding: .utf8)!
        try await task.send(.string(text))
        return (task, subId)
    }

    private func close(_ task: URLSessionWebSocketTask, subId: String) async {
        let close: [Any] = ["CLOSE", subId]
        if let data = try? JSONSerialization.data(withJSONObject: close),
           let text = String(data: data, encoding: .utf8) {
            _ = try? await task.send(.string(text))
        }
        task.cancel(with: .normalClosure, reason: nil)
    }

    private func drain(_ task: URLSessionWebSocketTask, subId: String, timeout: TimeInterval) async throws -> [RelayMessage] {
        var messages: [RelayMessage] = []
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            let msg: URLSessionWebSocketTask.Message
            do {
                msg = try await task.receive()
            } catch {
                break
            }
            switch msg {
            case .string(let text):
                guard let data = text.data(using: .utf8),
                      let arr = try? JSONSerialization.jsonObject(with: data) as? [Any],
                      let type = arr.first as? String else { continue }
                switch type {
                case "EVENT":
                    if arr.count >= 3, let event = arr[2] as? [String: Any] {
                        messages.append(.event(event))
                    }
                case "EOSE":
                    if arr.count >= 2, let sub = arr[1] as? String {
                        messages.append(.eose(sub))
                        await close(task, subId: subId)
                        return messages
                    }
                case "NOTICE":
                    if arr.count >= 2, let n = arr[1] as? String {
                        messages.append(.notice(n))
                    }
                default:
                    break
                }
            case .data:
                continue
            @unknown default:
                break
            }
        }
        await close(task, subId: subId)
        return messages
    }

    func fetchGroups() async throws -> [RawGroup] {
        let (task, subId) = try await openSubscription(["kinds": [39000, 39001, 39002, 39003]])
        let messages = try await drain(task, subId: subId, timeout: 30)
        var groups: [String: RawGroup] = [:]
        for msg in messages {
            guard case .event(let event) = msg else { continue }
            guard let kind = event["kind"] as? Int, (39000...39003).contains(kind) else { continue }
            guard let tags = event["tags"] as? [[Any]] else { continue }
            guard let id = tags.first(where: { $0.first as? String == "d" })?[safe: 1] as? String else { continue }
            let name = (event["content"] as? String) ?? id
            let parent = tags.first(where: { $0.first as? String == "a" })?[safe: 2] as? String
            groups[id] = RawGroup(id: id, name: name.isEmpty ? id : name, parent: parent)
        }
        return Array(groups.values)
    }

    /// Fetches the latest activity timestamp for each group id.
    /// Strategy: try a blanket `REQ kinds:[9]` first (relays that allow it);
    /// if no events come back, fall back to `REQ kinds:[9], "#h": groupIds`.
    func fetchActivity(groupIds: [String]) async throws -> [String: Date] {
        var activity = try await collectActivity(filter: ["kinds": [9]], groupIds: Set(groupIds), timeout: 20)
        if activity.isEmpty && !groupIds.isEmpty {
            activity = try await collectActivity(filter: ["kinds": [9], "#h": groupIds], groupIds: Set(groupIds), timeout: 20)
        }
        return activity
    }

    private func collectActivity(filter: [String: Any], groupIds: Set<String>, timeout: TimeInterval) async throws -> [String: Date] {
        let (task, subId) = try await openSubscription(filter)
        let messages = try await drain(task, subId: subId, timeout: timeout)
        var activity: [String: Date] = [:]
        var sawAnyEvent = false
        for msg in messages {
            guard case .event(let event) = msg else { continue }
            sawAnyEvent = true
            guard let createdAt = event["created_at"] as? Int else { continue }
            guard let tags = event["tags"] as? [[Any]] else { continue }
            let date = Date(timeIntervalSince1970: TimeInterval(createdAt))
            for tag in tags where tag.first as? String == "h" {
                if let h = tag[safe: 1] as? String, groupIds.contains(h) || groupIds.isEmpty {
                    activity[h] = max(activity[h] ?? .distantPast, date)
                }
            }
        }
        // Distinguish "no events" (need fallback) from "events but no h-tags matching"
        return sawAnyEvent ? activity : [:]
    }
}

private extension Array {
    subscript(safe index: Int) -> Element? {
        indices.contains(index) ? self[index] : nil
    }
}