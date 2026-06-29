import SwiftUI
import Foundation

/// Circular avatar for a Nostr pubkey. Shows the profile picture when the
/// host projection has it; falls back to a deterministic 5×5 symmetric
/// identicon derived from `pubkey` (same algorithm as `content-core`).
///
/// Replace `AsyncImage` with your own image cache (Kingfisher, Nuke, etc.)
/// if you already have one — the identicon fallback is self-contained.
///
/// Depends on `swiftui/user-avatar` for `ProfileWire` and `NostrProfileHost`.
public struct NostrAvatar: View {
    @Environment(\.nostrProfileHost) private var profileHost

    public let pubkey: String
    public let pictureUrl: URL?
    public let size: CGFloat
    public let consumerID: String?
    @State private var generatedConsumerID: String
    @State private var claimedPubkey: String?

    public init(
        pubkey: String,
        pictureUrl: URL? = nil,
        size: CGFloat = 40,
        consumerID: String? = nil
    ) {
        self.pubkey = pubkey
        self.pictureUrl = pictureUrl
        self.size = size
        self.consumerID = consumerID
        self._generatedConsumerID = State(
            initialValue: consumerID ?? "nostr-avatar.\(UUID().uuidString)"
        )
        self._claimedPubkey = State(initialValue: nil)
    }

    public init(profile: ProfileWire, size: CGFloat = 40) {
        self.pubkey = profile.pubkey
        self.pictureUrl = profile.avatarURL
        self.size = size
        self.consumerID = nil
        self._generatedConsumerID = State(
            initialValue: "nostr-avatar.static.\(UUID().uuidString)"
        )
        self._claimedPubkey = State(initialValue: nil)
    }

    public var body: some View {
        let url = pictureUrl ?? profileHost?.profile(forPubkey: pubkey)?.avatarURL

        Group {
            if let url {
                AsyncImage(url: url) { phase in
                    switch phase {
                    case .success(let image):
                        image.resizable().scaledToFill()
                    default:
                        identicon
                    }
                }
            } else {
                identicon
            }
        }
        .frame(width: size, height: size)
        .clipShape(Circle())
        .accessibilityHidden(true)
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
                profileHost?.releaseProfileRef(pubkey: claimedPubkey, consumerID: generatedConsumerID)
                self.claimedPubkey = nil
            }
        }
    }

    private var identicon: some View {
        NostrIdenticon.identiconView(forPubkey: pubkey, size: size)
    }
}

// MARK: - Identicon
//
// The bundled registry `NostrIdenticon` enum (the 5×5 symmetric pixel-grid
// identicon + djb2 palette) that `swiftui/user-avatar/NostrAvatar.swift` ships
// for STANDALONE installs is intentionally NOT re-declared here: this app also
// installs `swiftui/content-core`, whose `ContentTreeWire.swift` already vendors
// the identical `NostrIdenticon`. Re-declaring it would be a duplicate public
// symbol. `NostrAvatar` above reuses that shared one via
// `NostrIdenticon.identiconView(forPubkey:size:)`, so the avatar fallback stays
// in lock-step with the canonical content-core identicon (#2224 parity). This
// is the only deviation from the canonical user-avatar component, and it is the
// documented content-core coexistence dedup — not a behavioural fork.
