import Foundation

/// Bridges raw NIP-29 chat content to the renderable `ContentTreeWire` consumed
/// by `NostrContentView`, using the shared `nmp-content` substrate over the
/// generated `tokenizeContent` UniFFI function. This is the single place the
/// iOS shell turns wire content into a render tree — it holds zero nostr/NIP-21
/// parsing knowledge (mirrors the TUI's `tokenize_message`). Live entity
/// resolution (mention names, embedded-event cards) is layered on separately
/// via the `resolveProfileRef` / `resolveEventEmbed` seams.
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

    /// Tokenized tree memoized by a stable event id (preview path). The preview
    /// content is the same immutable event body as the timeline message, so the
    /// id-keyed cache is shared with `tree(for:)` — the group list and the open
    /// timeline tokenize each event exactly once.
    static func tree(forId id: String, content: String, kind: UInt32) -> ContentTreeWire? {
        guard !id.isEmpty else { return tokenize(content: content, kind: kind) }
        let key = id as NSString
        if let cached = cache.object(forKey: key) {
            return cached.tree
        }
        let tree = tokenize(content: content, kind: kind)
        cache.setObject(TreeBox(tree), forKey: key)
        return tree
    }

    /// Flatten a tokenized chat body to a single render-safe preview line for the
    /// group list (D5 previews). Raw `nostr:` entity tokens and bare URLs are
    /// never shown: mentions collapse to `@<label>` (resolved via `mentionLabel`,
    /// short-hex otherwise), embedded events to `[note]`, media to `[image]` /
    /// `[video]` / `[audio]`, and links to `[link]`. Falls back to the raw
    /// content when tokenization fails so a preview is never dropped.
    static func flattenedPreview(
        forId id: String,
        content: String,
        kind: UInt32,
        mentionLabel: (NostrWireUri) -> String
    ) -> String {
        guard let tree = tree(forId: id, content: content, kind: kind) else { return content }
        var out = ""
        for root in tree.roots {
            appendFlattened(node: root, tree: tree, mentionLabel: mentionLabel, into: &out)
        }
        let collapsed = out
            .split(whereSeparator: { $0.isWhitespace || $0.isNewline })
            .joined(separator: " ")
        return collapsed.isEmpty ? content : collapsed
    }

    private static func appendFlattened(
        node index: UInt32,
        tree: ContentTreeWire,
        mentionLabel: (NostrWireUri) -> String,
        into out: inout String
    ) {
        guard let node = tree.node(at: index) else { return }
        func recurse(_ children: [UInt32]) {
            for child in children {
                appendFlattened(node: child, tree: tree, mentionLabel: mentionLabel, into: &out)
            }
        }
        switch node {
        case .text(let value):
            out += value
        case .mention(let uri):
            out += "@\(mentionLabel(uri))"
        case .eventRef:
            out += "[note]"
        case .hashtag(let tag):
            out += "#\(tag)"
        case .url:
            out += "[link]"
        case .media(_, let kind):
            switch kind {
            case .image: out += "[image]"
            case .video: out += "[video]"
            case .audio: out += "[audio]"
            }
        case .image:
            out += "[image]"
        case .emoji(let shortcode, _):
            out += ":\(shortcode):"
        case .invoice:
            out += "[invoice]"
        case .inlineCode(let value):
            out += value
        case .codeBlock(_, let body):
            out += body
        case .softBreak, .hardBreak:
            out += " "
        case .paragraph(let children),
             .heading(_, let children),
             .blockQuote(let children),
             .emphasis(let children),
             .strong(let children):
            recurse(children)
        case .link(let children, _):
            // A markdown link with visible label text keeps the label; a bare
            // autolink (no children) collapses to the [link] chip.
            if children.isEmpty {
                out += "[link]"
            } else {
                recurse(children)
            }
        case .list(_, let items):
            for item in items { recurse(item) }
        case .rule:
            out += " "
        case .placeholder:
            break
        }
    }

    /// Pure tokenization over the `tokenizeContent` UniFFI function. Auto mode
    /// dispatches markdown vs plain by `kind` (kind 9/11 chat → plain).
    static func tokenize(content: String, kind: UInt32) -> ContentTreeWire? {
        // `tagsJson` is nil: the group-chat projection carries no tags, so there
        // is no NIP-30 emoji map to resolve here.
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
