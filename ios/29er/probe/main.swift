// Headless Swift probe: boots the TwentyNinerApp UniFFI facade exactly like
// the iOS app, sets an UpdateSink, signs in, opens group discovery, and
// prints every frame's projection keys + the decoded NDGS snapshot.
//
// Built as an iOS-sim command-line-style target reusing the same
// nmp_app_29er staticlib + Generated FlatBuffers readers the iOS app uses.
// Inlines the NDGS decode so it avoids the UI-entangled bridge glue.
import FlatBuffers
import Foundation

setvbuf(stdout, nil, _IONBF, 0)

let nsec = ProcessInfo.processInfo.environment["PROBE_NSEC"] ?? ""
let relay = ProcessInfo.processInfo.environment["PROBE_RELAY"] ?? "wss://nip29.f7z.io"
let waitSecs = UInt64(ProcessInfo.processInfo.environment["PROBE_WAIT"].flatMap(UInt64.init) ?? 15)

guard !nsec.isEmpty else {
    FileHandle.standardError.write("probe: PROBE_NSEC env var required\n".data(using: .utf8)!)
    exit(2)
}

func printFrame(_ frame: Data) {
    do {
        let decoded = try KernelUpdateFrameDecoder.decode(frame)
        switch decoded {
        case let .snapshot(schemaVer, sessionId, epoch, envelopes, rev, running, errToast, errCat):
            let keys = envelopes.map { "\($0.key)(\($0.fileIdentifier),v\($0.schemaVersion),\($0.payload.count)B)" }
            print("probe:   rev=\(rev) running=\(running) schema=\(schemaVer) sid=\(sessionId) epoch=\(epoch) err=\(errToast ?? "nil")/\(errCat ?? "nil")")
            print("probe:   projections=[\(keys.joined(separator: ", "))]")
            // Inline NDGS decode.
            if let ndgs = envelopes.first(where: { $0.key == "nmp.nip29.discovered_groups" && !$0.payload.isEmpty }) {
                var buf = ByteBuffer(data: ndgs.payload)
                let reader: nmp_nip29_DiscoveredGroupsSnapshot = getRoot(byteBuffer: &buf)
                let relayUrls = reader.hostRelayUrls.map { $0 ?? "" }
                let groups = reader.groups
                print("probe:   NDGS hostRelayUrls=\(relayUrls) groups=\(groups.count) schemaVer=\(reader.schemaVersion)")
                for g in groups.prefix(12) {
                    print("probe:     - id=\(g.groupId ?? "?") host=\(g.hostRelayUrl ?? "?") name=\(g.name ?? "nil") members=\(g.memberCount) public=\(g.public_) open=\(g.open_)")
                }
            } else {
                print("probe:   NDGS=nil (no nmp.nip29.discovered_groups sidecar in frame)")
            }
        case let .panic(msg):
            print("probe:   PANIC msg=\(msg)")
        }
    } catch {
        print("probe:   decode error: \(error)")
    }
}

print("probe: relay=\(relay)")
print("probe: constructing TwentyNinerApp")
let app = TwentyNinerApp()

// Wire the keyring capability handler (Keychain-backed) exactly like the iOS
// app's KernelBridge.init — without it the actor's keyring writes fail with
// core_keyring_write_failed on every frame.
let capabilities = TwentyNinerCapabilities()
capabilities.start()
app.setCapabilityCallback(sink: capabilities)

// Point the runtime at Application Support/NMP for persistent LMDB storage
// (mirrors KernelBridge.configureStoragePath). Without it the runtime is
// in-memory, which also works but doesn't exercise the real path.
if let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first {
    let dir = base.appendingPathComponent("NMP", isDirectory: true)
    try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    let ok = app.setStoragePath(path: dir.path)
    print("probe: setStoragePath(\(dir.path)) -> \(ok)")
} else {
    print("probe: no Application Support dir — running in-memory")
}

print("probe: seeding default relays")
let seeded = app.seedDefaultRelays()
print("probe: seedDefaultRelays -> \(seeded)")

print("probe: declaring consumed projections")
app.declareConsumedProjections()
// NOTE: deliberately NOT calling declareIncrementalApply() — testing whether
// the D3-3 cache-merge layer is filtering out the NDGS sidecar.
// _ = app.declareIncrementalApply()

final class ProbeSink: UpdateSink, @unchecked Sendable {
    func onUpdate(frame: Data) {
        print("probe: frame bytes=\(frame.count)")
        printFrame(frame)
    }
}

print("probe: setting update sink")
app.setUpdateSink(sink: ProbeSink())

print("probe: start(visibleLimit:256, emitHz:12)")
app.start(visibleLimit: 256, emitHz: 12)

print("probe: signinNsec")
app.signinNsec(nsec: nsec, makeActive: true)

Thread.sleep(forTimeInterval: 1.5)

print("probe: openGroupDiscovery(\(relay))")
let opened = app.openGroupDiscovery(hostRelayUrl: relay)
print("probe: openGroupDiscovery -> \(opened)")
print("probe: isAlive after open -> \(app.isAlive())")

print("probe: dispatch nmp.nip29.discover for \(relay)")
let discover = app.dispatchNip29Action(
    namespace: "nmp.nip29.discover",
    bodyJson: "{\"relay_url\":\"\(relay)\"}"
)
print("probe: discover -> error=\(discover.error ?? "nil") correlationId=\(discover.correlationId ?? "nil")")
print("probe: isAlive after discover -> \(app.isAlive())")

print("probe: waiting \(waitSecs)s for frames...")
Thread.sleep(forTimeInterval: TimeInterval(waitSecs))
print("probe: isAlive at end -> \(app.isAlive())")

print("probe: done")
app.shutdown()
exit(0)