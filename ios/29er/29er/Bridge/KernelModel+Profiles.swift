import Foundation

@MainActor
extension KernelModel: NostrProfileHost {
    func profile(forPubkey pubkey: String) -> ProfileWire? {
        profileRefs.profile(forPubkey: pubkey)
    }

    func resolveProfileRef(pubkey: String, consumerID: String) {
        kernel.resolveProfileRef(pubkey: pubkey, consumerID: consumerID)
    }

    func releaseProfileRef(pubkey: String, consumerID: String) {
        kernel.releaseProfileRef(pubkey: pubkey, consumerID: consumerID)
    }
}
