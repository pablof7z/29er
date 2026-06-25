import XCTest

final class M002RelayFlowUITests: XCTestCase {
    private let joinableGroup = "m002-joinable"
    private let leavableGroup = "m002-leavable"
    private let adminGroup = "m002-admin-root"
    private let altParentGroup = "m002-alt-parent"
    private let movableGroup = "m002-movable"
    private let childGroup = "m002-child-ui"
    private let inviteCode = "m002-code-ui"

    private var relayURL: URL!
    private var appNsec: String!
    private var targetPubkey: String!
    private var seedEvents: [RelayEvent] = []
    private var projectionEvents: ProjectionEvents!

    override func setUpWithError() throws {
        continueAfterFailure = false

        let env = ProcessInfo.processInfo.environment
        let config = try UATConfig.load(from: env)
        guard let relayURL = URL(string: config.relayURL)
        else {
            throw XCTSkip("M002 UAT relay URL is invalid")
        }

        self.relayURL = relayURL
        appNsec = config.nsec
        targetPubkey = config.targetPubkey
        seedEvents = config.seedEvents
        projectionEvents = config.projectionEvents
    }

    func testM002RelayActionsPublishToRelay() throws {
        let app = XCUIApplication()
        app.launchEnvironment["M001_DEFAULT_RELAY_URL"] = relayURL.absoluteString
        app.launchEnvironment["S03_AUTO_SIGN_IN_NSEC"] = appNsec
        app.launch()

        XCTAssertTrue(app.staticTexts["No Rooms"].waitForExistence(timeout: 15), "App did not reach empty group tree")
        publishSeedEvents(seedEvents)

        tap("group-row-\(joinableGroup)", in: app, timeout: 20)
        tap("join-button-\(joinableGroup)", in: app)
        type("join-reason-field", text: "joining from M002 UI UAT", in: app)
        tap("join-submit-button", in: app)
        XCTAssertTrue(
            waitForRelayEvent(kind: 9021, h: joinableGroup) { event in
                event.content == "joining from M002 UI UAT"
            }.hasTag(["h", joinableGroup])
        )
        publishRelayEvent(projectionEvents.joined)
        XCTAssertTrue(app.descendants(matching: .any)["leave-button-\(joinableGroup)"].waitForExistence(timeout: 10))
        back(app)

        tap("group-row-\(leavableGroup)", in: app)
        tap("leave-button-\(leavableGroup)", in: app)
        type("leave-reason-field", text: "leaving from M002 UI UAT", in: app)
        tap("leave-submit-button", in: app)
        XCTAssertTrue(
            waitForRelayEvent(kind: 9022, h: leavableGroup) { event in
                event.content == "leaving from M002 UI UAT"
            }.hasTag(["h", leavableGroup])
        )
        publishRelayEvent(projectionEvents.left)
        XCTAssertTrue(app.descendants(matching: .any)["join-button-\(leavableGroup)"].waitForExistence(timeout: 10))
        back(app)

        tap("group-row-\(adminGroup)", in: app)
        tap("admin-button-\(adminGroup)", in: app)
        type("admin-invite-codes-field", text: inviteCode, in: app)
        tap("admin-create-invite-button", in: app)
        XCTAssertTrue(
            waitForRelayEvent(kind: 9009, h: adminGroup) { event in
                event.hasTag(named: "code")
            }.hasTag(["h", adminGroup])
        )

        tapVisibleControl(named: "People", in: app)
        type("admin-target-pubkey-field", text: targetPubkey, in: app)
        type("admin-role-field", text: "member", in: app)
        type("admin-reason-field", text: "adding from M002 UI UAT", in: app)
        tap("admin-add-user-button", in: app)
        XCTAssertTrue(
            waitForRelayEvent(kind: 9000, h: adminGroup) { event in
                event.hasTag(["p", self.targetPubkey, "member"]) &&
                    event.hasTag(["reason", "adding from M002 UI UAT"])
            }.hasTag(["h", adminGroup])
        )
        publishRelayEvent(projectionEvents.adminMembers)
        XCTAssertTrue(app.buttons["2 members"].waitForExistence(timeout: 10))

        tapVisibleControl(named: "Room", in: app)
        scrollTo("admin-child-local-id-field", in: app)
        type("admin-child-local-id-field", text: childGroup, in: app)
        type("admin-child-name-field", text: "M002 Child UI", in: app)
        type("admin-child-about-field", text: "created by M002 UI UAT", in: app)
        tap("admin-create-child-button", in: app)
        XCTAssertTrue(waitForRelayEvent(kind: 9007, h: childGroup).hasTag(["h", childGroup]))
        XCTAssertTrue(
            waitForRelayEvent(kind: 9002, h: childGroup) { event in
                event.hasTag(["parent", self.adminGroup]) &&
                    event.hasTag(["name", "M002 Child UI"])
            }.hasTag(["h", childGroup])
        )
        publishRelayEvent(projectionEvents.adminRootChild)
        publishRelayEvent(projectionEvents.child)

        tapVisibleButton(named: "Done", in: app)
        tap("group-children-\(adminGroup)", in: app, timeout: 15)
        tap("group-row-\(childGroup)", in: app, timeout: 10)
        back(app)
        back(app)
        back(app)

        tap("group-row-\(movableGroup)", in: app)
        tap("admin-button-\(movableGroup)", in: app)
        tapVisibleControl(named: "Move", in: app)
        scrollTo("admin-parent-option-\(altParentGroup)", in: app)
        tap("admin-parent-option-\(altParentGroup)", in: app)
        tap("admin-set-parent-button", in: app)
        XCTAssertTrue(
            waitForRelayEvent(kind: 9002, h: movableGroup) { event in
                event.hasTag(["parent", "m002-alt-parent"])
            }.hasTag(["h", movableGroup])
        )
        publishRelayEvent(projectionEvents.altParentChild)
        publishRelayEvent(projectionEvents.moved)
        tapVisibleButton(named: "Done", in: app)
        back(app)
        tap("group-row-\(altParentGroup)", in: app, timeout: 15)
        tap("group-children-\(altParentGroup)", in: app, timeout: 15)
        XCTAssertTrue(app.descendants(matching: .any)["group-row-\(movableGroup)"].waitForExistence(timeout: 10))
    }

    private func tap(_ identifier: String, in app: XCUIApplication, timeout: TimeInterval = 10) {
        let element = app.descendants(matching: .any)[identifier].firstMatch
        XCTAssertTrue(element.waitForExistence(timeout: timeout), "Missing \(identifier)")
        element.tap()
    }

    private func type(_ identifier: String, text: String, in app: XCUIApplication) {
        scrollTo(identifier, in: app)
        let element = app.descendants(matching: .any)[identifier].firstMatch
        XCTAssertTrue(element.waitForExistence(timeout: 5), "Missing \(identifier)")
        element.tap()
        element.typeText(text)
    }

    private func scrollTo(_ identifier: String, in app: XCUIApplication) {
        let element = app.descendants(matching: .any)[identifier].firstMatch
        var attempts = 0
        while !element.exists && attempts < 8 {
            app.swipeUp()
            attempts += 1
        }
    }

    private func tapVisibleButton(named name: String, in app: XCUIApplication) {
        let button = app.buttons[name].firstMatch
        XCTAssertTrue(button.waitForExistence(timeout: 5), "Missing button \(name)")
        button.tap()
    }

    private func tapVisibleControl(named name: String, in app: XCUIApplication) {
        let button = app.buttons[name].firstMatch
        if button.waitForExistence(timeout: 3) {
            button.tap()
            return
        }
        let text = app.staticTexts[name].firstMatch
        XCTAssertTrue(text.waitForExistence(timeout: 5), "Missing control \(name)")
        text.tap()
    }

    private func tapVisibleText(_ text: String, in app: XCUIApplication) {
        let element = app.staticTexts[text].firstMatch
        XCTAssertTrue(element.waitForExistence(timeout: 5), "Missing text \(text)")
        element.tap()
    }

    private func back(_ app: XCUIApplication) {
        let start = app.coordinate(withNormalizedOffset: CGVector(dx: 0.02, dy: 0.5))
        let end = app.coordinate(withNormalizedOffset: CGVector(dx: 0.72, dy: 0.5))
        start.press(forDuration: 0.05, thenDragTo: end)
    }

    private func waitForRelayEvent(
        kind: Int,
        h: String,
        timeout: TimeInterval = 10,
        matching: @escaping (RelayEvent) -> Bool = { _ in true }
    ) -> RelayEvent {
        let expectation = XCTestExpectation(description: "kind \(kind) h \(h)")
        let task = URLSession.shared.webSocketTask(with: relayURL)
        var matchedEvent: RelayEvent?
        var capturedError: Error?

        func receive() {
            task.receive { result in
                switch result {
                case .failure(let error):
                    capturedError = error
                    expectation.fulfill()
                case .success(let message):
                    do {
                        if let event = try RelayEvent.decode(from: message),
                           event.kind == kind,
                           event.hasTag(["h", h]),
                           matching(event) {
                            matchedEvent = event
                            expectation.fulfill()
                        } else {
                            receive()
                        }
                    } catch {
                        capturedError = error
                        expectation.fulfill()
                    }
                }
            }
        }

        task.resume()
        let request: [Any] = [
            "REQ",
            "m002-ui-\(kind)-\(h)",
            ["kinds": [kind], "#h": [h], "limit": 20],
        ]
        let data = try! JSONSerialization.data(withJSONObject: request)
        let raw = String(data: data, encoding: .utf8)!
        task.send(.string(raw)) { error in
            if let error {
                capturedError = error
                expectation.fulfill()
            } else {
                receive()
            }
        }

        wait(for: [expectation], timeout: timeout)
        task.cancel(with: .goingAway, reason: nil)

        if let error = capturedError {
            XCTFail("Relay watch failed for kind \(kind) h \(h): \(error)")
        }
        guard let matchedEvent else {
            XCTFail("Timed out waiting for kind \(kind) h \(h)")
            return RelayEvent.empty
        }
        return matchedEvent
    }

    private func publishSeedEvents(_ events: [RelayEvent]) {
        for event in events {
            publishRelayEvent(event)
        }
    }

    private func publishRelayEvent(_ event: RelayEvent) {
        let expectation = XCTestExpectation(description: "publish seed \(event.kind)")
        let task = URLSession.shared.webSocketTask(with: relayURL)
        var capturedError: Error?

        func receiveOK() {
            task.receive { result in
                switch result {
                case .failure(let error):
                    capturedError = error
                    expectation.fulfill()
                case .success(let message):
                    do {
                        if try RelayOK.decode(from: message, expectedEventId: event.id) {
                            expectation.fulfill()
                        } else {
                            receiveOK()
                        }
                    } catch {
                        capturedError = error
                        expectation.fulfill()
                    }
                }
            }
        }

        task.resume()
        let eventData = try! JSONEncoder().encode(event)
        let eventJSON = String(data: eventData, encoding: .utf8)!
        task.send(.string("[\"EVENT\",\(eventJSON)]")) { error in
            if let error {
                capturedError = error
                expectation.fulfill()
            } else {
                receiveOK()
            }
        }
        wait(for: [expectation], timeout: 5)
        task.cancel(with: .goingAway, reason: nil)

        if let capturedError {
            XCTFail("Could not publish seed event kind \(event.kind): \(capturedError)")
        }
    }
}

private struct UATConfig: Decodable {
    let relayURL: String
    let nsec: String
    let targetPubkey: String
    let seedEvents: [RelayEvent]
    let projectionEvents: ProjectionEvents

    static func load(from environment: [String: String]) throws -> UATConfig {
        if let relayURL = environment["M002_UAT_RELAY_URL"],
           let nsec = environment["M002_UAT_NSEC"],
           let targetPubkey = environment["M002_UAT_TARGET_PUBKEY"] {
            return UATConfig(
                relayURL: relayURL,
                nsec: nsec,
                targetPubkey: targetPubkey,
                seedEvents: [],
                projectionEvents: .empty
            )
        }

        let path = environment["M002_UAT_CONFIG_PATH"] ?? "/tmp/29er-m002-uat-env.json"
        let url = URL(fileURLWithPath: path)
        guard FileManager.default.fileExists(atPath: url.path) else {
            throw XCTSkip("M002 UAT config missing at \(url.path)")
        }
        let data = try Data(contentsOf: url)
        return try JSONDecoder().decode(UATConfig.self, from: data)
    }
}

private struct ProjectionEvents: Decodable {
    let joined: RelayEvent
    let left: RelayEvent
    let adminMembers: RelayEvent
    let adminRootChild: RelayEvent
    let child: RelayEvent
    let altParentChild: RelayEvent
    let moved: RelayEvent

    static let empty = ProjectionEvents(
        joined: .empty,
        left: .empty,
        adminMembers: .empty,
        adminRootChild: .empty,
        child: .empty,
        altParentChild: .empty,
        moved: .empty
    )
}

private struct RelayEvent: Codable {
    let id: String
    let pubkey: String
    let createdAt: Int
    let kind: Int
    let tags: [[String]]
    let content: String
    let sig: String

    enum CodingKeys: String, CodingKey {
        case id
        case pubkey
        case createdAt = "created_at"
        case kind
        case tags
        case content
        case sig
    }

    static let empty = RelayEvent(id: "", pubkey: "", createdAt: 0, kind: 0, tags: [], content: "", sig: "")

    func hasTag(_ expected: [String]) -> Bool {
        tags.contains { tag in
            tag.count >= expected.count && Array(tag.prefix(expected.count)) == expected
        }
    }

    func hasTag(named name: String) -> Bool {
        tags.contains { tag in
            tag.first == name
        }
    }

    static func decode(from message: URLSessionWebSocketTask.Message) throws -> RelayEvent? {
        let raw: String
        switch message {
        case .string(let string):
            raw = string
        case .data(let data):
            raw = String(decoding: data, as: UTF8.self)
        @unknown default:
            return nil
        }

        guard let envelope = try JSONSerialization.jsonObject(with: Data(raw.utf8)) as? [Any],
              envelope.count >= 3,
              envelope[0] as? String == "EVENT",
              let eventObject = envelope[2] as? [String: Any]
        else {
            return nil
        }

        let eventData = try JSONSerialization.data(withJSONObject: eventObject)
        return try JSONDecoder().decode(RelayEvent.self, from: eventData)
    }
}

private enum RelayOK {
    static func decode(from message: URLSessionWebSocketTask.Message, expectedEventId: String) throws -> Bool {
        let raw: String
        switch message {
        case .string(let string):
            raw = string
        case .data(let data):
            raw = String(decoding: data, as: UTF8.self)
        @unknown default:
            return false
        }

        guard let envelope = try JSONSerialization.jsonObject(with: Data(raw.utf8)) as? [Any],
              envelope.count >= 3,
              envelope[0] as? String == "OK",
              envelope[1] as? String == expectedEventId,
              envelope[2] as? Bool == true
        else {
            return false
        }
        return true
    }
}
