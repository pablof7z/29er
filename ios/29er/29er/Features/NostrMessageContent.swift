import Foundation

/// Bridges raw NIP-29 chat content to the renderable `ContentTreeWire` consumed
/// by `NostrContentView`, using the shared `nmp-content` substrate over the
/// generated 29er UniFFI facade. This is the single place the iOS shell turns
/// wire content into a render tree, and it holds zero nostr/NIP-21 parsing
/// knowledge (mirrors the TUI's `tokenize_message`). Live entity resolution
/// (mention names, embedded-event cards) is layered on separately via the
/// facade's typed ref doors.
enum NostrMessageContent {
    /// `nmp-content` render mode: 0 = plain, 1 = markdown, 2 = auto (by kind).
    private static let modeAuto: Int32 = 2

    /// Process-wide cache keyed by event id. Content for a given event id is
    /// immutable, so a tokenized tree never goes stale; `NSCache` is internally
    /// thread-safe, so `nonisolated(unsafe)` is sound under Swift 6 strict
    /// concurrency and self-evicting under memory pressure.
    nonisolated(unsafe) private static let cache = NSCache<NSString, TreeBox>()

    /// Reference box so an absent-but-tokenized entry (a `nil` tree from a parse
    /// fallback) is still cacheable and not re-tokenized every render.
    private final class TreeBox {
        let tree: ContentTreeWire?
        init(_ tree: ContentTreeWire?) { self.tree = tree }
    }

    /// Tokenized tree for a chat message, memoized by event id. Returns `nil`
    /// only when tokenization fails — callers fall back to raw text so no
    /// message is ever dropped.
    static func tree(for message: GroupChatMessage) -> ContentTreeWire? {
        let key = message.id as NSString
        if let cached = cache.object(forKey: key) {
            return cached.tree
        }
        let tree = tokenize(content: message.content, kind: message.kind)
        cache.setObject(TreeBox(tree), forKey: key)
        return tree
    }

    /// Pure tokenization through the 29er UniFFI facade. Auto mode dispatches
    /// markdown vs plain by `kind` (kind 9/11 chat -> plain).
    static func tokenize(content: String, kind: UInt32) -> ContentTreeWire? {
        // `tagsJson` is nil: the group-chat projection carries no tags, so
        // there is no NIP-30 emoji map to resolve here.
        let json = tokenizeContent(content: content, tagsJson: nil, mode: modeAuto, kind: kind)
        guard let data = json.data(using: .utf8) else { return nil }
        return try? JSONDecoder().decode(TokenizeResult.self, from: data).tree
    }

    /// Wire shape of the `tokenizeContent` success payload.
    private struct TokenizeResult: Decodable {
        let ok: Bool
        let tree: ContentTreeWire?
    }
}
