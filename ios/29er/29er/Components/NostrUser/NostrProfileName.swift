import SwiftUI

/// Text view rendering a Nostr pubkey's display name from the
/// `\.nostrProfileHost` projection. The registry sibling of `NostrAvatar`:
/// both read the same profile host and own their own ref lifecycle — this view
/// claims a profile ref on appear (so the kernel resolves the pubkey's kind:0)
/// and releases it on disappear.
///
/// Until the profile resolves it falls back to the Rust-truncated short id —
/// identity is never reformatted in Swift (aim.md §6.9). Raw protocol values
/// only (D11): the projection owns the name, the shell only renders it.
public struct NostrProfileName: View {
    @Environment(\.nostrProfileHost) private var profileHost

    public let pubkey: String
    public let consumerID: String?
    private let font: Font
    @State private var generatedConsumerID: String
    @State private var claimedPubkey: String?

    public init(
        pubkey: String,
        consumerID: String? = nil,
        font: Font = .body
    ) {
        self.pubkey = pubkey
        self.consumerID = consumerID
        self.font = font
        self._generatedConsumerID = State(
            initialValue: consumerID ?? "nostr-profile-name.\(UUID().uuidString)"
        )
        self._claimedPubkey = State(initialValue: nil)
    }

    public var body: some View {
        Text(displayLabel)
            .font(font)
            .task(id: pubkey) {
                await MainActor.run {
                    if let claimedPubkey, claimedPubkey != pubkey {
                        profileHost?.releaseProfileRef(
                            pubkey: claimedPubkey,
                            consumerID: generatedConsumerID
                        )
                    }
                    claimedPubkey = pubkey
                    profileHost?.resolveProfileRef(pubkey: pubkey, consumerID: generatedConsumerID)
                }
            }
            .onDisappear {
                if let claimedPubkey {
                    profileHost?.releaseProfileRef(
                        pubkey: claimedPubkey,
                        consumerID: generatedConsumerID
                    )
                    self.claimedPubkey = nil
                }
            }
    }

    /// `displayName` (else Rust-truncated `npubShort`) once the kind:0 has
    /// resolved; the short hex of the raw pubkey as the pre-resolution fallback.
    private var displayLabel: String {
        profileHost?.profile(forPubkey: pubkey)?.display ?? pubkey.shortHex
    }
}
