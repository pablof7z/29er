import FlatBuffers
import Foundation

/// HAND-WRITTEN glue between the `flatc --swift` FlatBuffers reader structs
/// and the 29er domain types, for the typed-projection-sidecar decode path.
///
/// Mirrors Chirp's `TypedProjectionGlue.swift` but strips to the S01 surface:
/// only the `nmp.nip29.discovered_groups` (`NDGS`) and `active_account`
/// (`KACT`) sidecars. New projection keys are added here as 29er grows.
///
/// Raw protocol values only (D11 — no display helpers). Each function takes
/// the generated reader struct and returns the SAME 29er domain value the
/// generic JSON `payload` path would yield for that key.
enum TypedProjectionGlue {
    // MARK: active_account → String?

    /// Map the typed `active_account` sidecar (`KACT` /
    /// `nmp_kernel_ActiveAccountSnapshot`) to the `String?` the JSON
    /// `projections.active_account` path yields — `nil` when no account is
    /// active (`has_active_account == false` mirrors JSON `null`).
    static func activeAccount(_ reader: nmp_kernel_ActiveAccountSnapshot) -> String? {
        reader.hasActiveAccount ? (reader.pubkey ?? "") : nil
    }

    // MARK: app.29er.group_chat -> GroupChatSnapshot

    static func groupChat(_ reader: nmp_app_29er_GroupChatSnapshot) -> GroupChatSnapshot {
        GroupChatSnapshot(
            messages: reader.messages.map { row in
                GroupChatMessage(
                    id: row.id ?? "",
                    pubkey: row.pubkey ?? "",
                    rawContent: row.rawContent ?? "",
                    copyText: row.copyText ?? "",
                    createdAt: row.createdAt,
                    kind: row.kind,
                    contentTree: contentTree(fromNFCTBytes: row.contentTreeBytes.map { $0 }),
                    mentionPubkeys: row.mentionPubkeys.map { $0 ?? "" },
                    eventRefUris: row.eventRefUris.map { $0 ?? "" },
                    eventRefPrimaryIds: row.eventRefPrimaryIds.map { $0 ?? "" }
                )
            },
            profileDemandPubkeys: reader.profileDemandPubkeys.map { $0 ?? "" },
            eventRefUris: reader.eventRefUris.map { $0 ?? "" },
            eventRefPrimaryIds: reader.eventRefPrimaryIds.map { $0 ?? "" }
        )
    }

    // MARK: refs.event.envelopes -> [String: EmbeddedEventEnvelope]

    static func refEventEnvelopes(
        _ reader: nmp_embed_RefEventEnvelopes
    ) -> [String: EmbeddedEventEnvelope] {
        Dictionary(uniqueKeysWithValues: reader.entries.compactMap { row in
            guard let envelope = embeddedEventEnvelope(row) else { return nil }
            return (envelope.primaryId, envelope)
        })
    }

    private static func embeddedEventEnvelope(
        _ row: nmp_embed_EmbeddedEventEnvelope
    ) -> EmbeddedEventEnvelope? {
        guard let projectionReader = row.projection else { return nil }
        let primaryId = row.primaryId ?? ""
        guard !primaryId.isEmpty else { return nil }
        return EmbeddedEventEnvelope(
            uri: row.uri ?? "",
            primaryId: primaryId,
            depth: row.depth,
            maxDepth: row.maxDepth,
            projection: embedProjection(projectionReader),
            collapsed: row.collapsed,
            collapseReason: row.hasCollapseReason ? row.collapseReason : nil
        )
    }

    private static func embedProjection(
        _ reader: nmp_embed_EmbedKindProjection
    ) -> EmbedKindProjection {
        switch reader.kind {
        case .shortnote:
            guard let note = reader.shortNote else {
                return .unknown(UnknownProjection(kind: 1, authorPubkey: ""))
            }
            let contentTreeBytes = note.contentTree.map { $0 }
            return .shortNote(ShortNoteProjection(
                id: note.id ?? "",
                authorPubkey: note.authorPubkey ?? "",
                createdAt: note.createdAt,
                content: contentPlainText(fromNFCTBytes: contentTreeBytes),
                mediaUrls: note.mediaUrls.map { $0 ?? "" }
            ))
        case .article:
            guard let article = reader.article else {
                return .unknown(UnknownProjection(kind: 30023, authorPubkey: ""))
            }
            return .article(ArticleProjection(
                id: article.id ?? "",
                authorPubkey: article.authorPubkey ?? "",
                createdAt: article.createdAt,
                title: article.hasTitle ? article.title : nil,
                summary: article.hasSummary ? article.summary : nil,
                heroImageUrl: article.hasHeroImageUrl ? article.heroImageUrl : nil,
                dTag: article.dTag ?? "",
                content: contentPlainText(fromNFCTBytes: article.contentTree.map { $0 })
            ))
        case .highlight:
            guard let highlight = reader.highlight else {
                return .unknown(UnknownProjection(kind: 9802, authorPubkey: ""))
            }
            return .highlight(HighlightProjection(
                id: highlight.id ?? "",
                authorPubkey: highlight.authorPubkey ?? "",
                createdAt: highlight.createdAt,
                highlightedText: highlight.highlightedText ?? "",
                sourceEventId: highlight.hasSourceEventId ? highlight.sourceEventId : nil,
                sourceEventAddr: highlight.hasSourceEventAddr ? highlight.sourceEventAddr : nil,
                sourceUrl: highlight.hasSourceUrl ? highlight.sourceUrl : nil,
                context: highlight.hasContext ? highlight.context : nil
            ))
        case .profile:
            guard let profile = reader.profile else {
                return .unknown(UnknownProjection(kind: 0, authorPubkey: ""))
            }
            return .profile(ProfileProjection(
                pubkey: profile.pubkey ?? "",
                displayName: profile.hasDisplayName ? profile.displayName : nil,
                pictureUrl: profile.hasPictureUrl ? profile.pictureUrl : nil,
                about: profile.hasAbout ? profile.about : nil,
                nip05: profile.hasNip05 ? profile.nip05 : nil,
                lud16: profile.hasLud16 ? profile.lud16 : nil,
                bannerUrl: profile.hasBannerUrl ? profile.bannerUrl : nil
            ))
        case .unknown:
            guard let unknown = reader.unknown else {
                return .unknown(UnknownProjection(kind: 0, authorPubkey: ""))
            }
            return .unknown(UnknownProjection(
                kind: unknown.kind,
                authorPubkey: unknown.authorPubkey ?? "",
                createdAt: unknown.createdAt,
                content: unknown.content ?? contentPlainText(fromNFCTBytes: unknown.contentTree.map { $0 }),
                tags: unknown.tags.map { tag in tag.values.map { $0 ?? "" } },
                altText: unknown.hasAltText ? unknown.altText : nil
            ))
        }
    }

    private static func contentTree(fromNFCTBytes bytes: [UInt8]) -> ContentTreeWire? {
        guard !bytes.isEmpty else { return nil }
        var buffer = ByteBuffer(data: Data(bytes))
        let reader: nmp_content_ContentTreeWire = getRoot(byteBuffer: &buffer)
        return contentTree(reader)
    }

    private static func contentPlainText(fromNFCTBytes bytes: [UInt8]) -> String {
        guard let tree = contentTree(fromNFCTBytes: bytes) else { return "" }
        return nostrContentPlainText(
            tree,
            children: tree.roots,
            mentionLabel: NostrContentView.defaultMentionLabel
        ).trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private static func contentTree(_ reader: nmp_content_ContentTreeWire) -> ContentTreeWire {
        ContentTreeWire(
            nodes: reader.nodes.map(nostrNode),
            roots: reader.roots.map { $0 },
            mode: renderMode(reader.mode)
        )
    }

    private static func nostrNode(_ row: nmp_content_WireNode) -> NostrWireNode {
        switch row.kind {
        case .text:
            return .text(row.text ?? "")
        case .mention:
            guard let uri = row.nostrUri else { return .placeholder(reason: .unresolvedUri) }
            return .mention(nostrUri(uri))
        case .eventref:
            guard let uri = row.nostrUri else { return .placeholder(reason: .unresolvedUri) }
            return .eventRef(nostrUri(uri))
        case .hashtag:
            return .hashtag(row.tag ?? "")
        case .url:
            return .url(row.url ?? "")
        case .media:
            return .media(urls: row.mediaUrls.map { $0 ?? "" }, kind: mediaKind(row.mediaKind))
        case .emoji:
            return .emoji(shortcode: row.shortcode ?? "", url: row.emojiUrl)
        case .invoice:
            return .invoice(invoiceKind(row.invoiceKind, payload: row.invoicePayload ?? ""))
        case .heading:
            return .heading(level: row.level, children: row.children.map { $0 })
        case .paragraph:
            return .paragraph(children: row.children.map { $0 })
        case .blockquote:
            return .blockQuote(children: row.children.map { $0 })
        case .codeblock:
            return .codeBlock(info: row.codeInfo, body: row.text ?? "")
        case .list:
            let orderedStart = row.orderedStart >= 0 ? UInt64(row.orderedStart) : nil
            return .list(
                orderedStart: orderedStart,
                items: row.listItems.map { $0.children.map { $0 } }
            )
        case .rule:
            return .rule
        case .emphasis:
            return .emphasis(children: row.children.map { $0 })
        case .strong:
            return .strong(children: row.children.map { $0 })
        case .inlinecode:
            return .inlineCode(row.text ?? "")
        case .link:
            return .link(children: row.children.map { $0 }, href: row.href)
        case .image:
            return .image(alt: row.alt ?? "", title: row.imgTitle, src: row.url)
        case .softbreak:
            return .softBreak
        case .hardbreak:
            return .hardBreak
        case .placeholder:
            return .placeholder(reason: placeholderReason(row.placeholderReason))
        }
    }

    private static func nostrUri(_ row: nmp_content_WireNostrUri) -> NostrWireUri {
        NostrWireUri(
            uri: row.uri ?? "",
            kind: uriKind(row.kind),
            primaryId: row.primaryId ?? "",
            relays: row.relays.map { $0 ?? "" },
            author: row.author,
            eventKind: row.eventKind == 0 ? nil : row.eventKind
        )
    }

    private static func uriKind(_ kind: nmp_content_WireNostrUriKind) -> NostrWireUriKind {
        switch kind {
        case .profile: return .profile
        case .event: return .event
        case .address: return .address
        }
    }

    private static func mediaKind(_ raw: UInt8) -> NostrMediaKind {
        switch raw {
        case 1: return .video
        case 2: return .audio
        default: return .image
        }
    }

    private static func invoiceKind(_ raw: UInt8, payload: String) -> NostrWireInvoice {
        switch raw {
        case 1: return .bolt12(payload)
        case 2: return .cashu(payload)
        default: return .bolt11(payload)
        }
    }

    private static func placeholderReason(
        _ reason: nmp_content_PlaceholderReason
    ) -> NostrWirePlaceholderReason {
        switch reason {
        case .unresolveduri: return .unresolvedUri
        case .depthlimit: return .depthLimit
        }
    }

    private static func renderMode(_ mode: nmp_content_RenderMode) -> String {
        switch mode {
        case .auto: return "Auto"
        case .markdown: return "Markdown"
        case .text: return "Plain"
        }
    }

    // MARK: nmp.nip29.group_roster -> GroupRosterSnapshot

    static func groupRoster(_ reader: nmp_nip29_GroupRosterSnapshot) -> GroupRosterSnapshot {
        GroupRosterSnapshot(
            hostRelayUrl: reader.hostRelayUrl ?? "",
            groupId: reader.groupId,
            members: reader.members.map { row in
                GroupRosterMember(
                    pubkey: row.pubkey ?? "",
                    roles: row.roles.map { $0 ?? "" },
                    isAdmin: row.isAdmin,
                    isMember: row.isMember
                )
            },
            roles: reader.roles.map { role in
                GroupRole(name: role.name ?? "", description: role.description)
            }
        )
    }

    // MARK: publish_outbox → [PublishOutboxItem]

    static func publishOutbox(_ reader: nmp_kernel_PublishOutboxSnapshot) -> [PublishOutboxItem] {
        reader.items.map { item in
            PublishOutboxItem(
                handle: item.handle ?? "",
                eventId: item.eventId ?? "",
                kind: item.kind,
                content: item.content ?? "",
                createdAt: item.createdAt,
                status: item.status ?? "",
                canRetry: item.canRetry,
                targetRelays: Int(item.targetRelays),
                relays: item.relays.map { relay in
                    PublishOutboxRelay(
                        relayUrl: relay.relayUrl ?? "",
                        status: relay.status ?? "",
                        attempt: relay.attempt,
                        message: relay.message ?? "",
                        relayReason: relay.relayReason ?? ""
                    )
                }
            )
        }
    }

    // MARK: nmp.nip29.discovered_groups → DiscoveredGroupsSnapshot

    /// Map the typed `nmp.nip29.discovered_groups` sidecar (`NDGS` /
    /// `nmp_nip29_DiscoveredGroupsSnapshot`) to the `DiscoveredGroupsSnapshot`
    /// the JSON `projections["nmp.nip29.discovered_groups"]` path yields. Flat
    /// field-for-field copy: a top-level `hostRelayUrl` plus one ordered
    /// `[DiscoveredGroup]` vector (alphabetical by `groupId`; Rust owns the
    /// order). `name`/`picture`/`about`/`parent` are tag-derived
    /// `Option<String>` on the wire — bare FlatBuffers strings where absent
    /// decodes to `nil`; the glue preserves that `nil` (NOT `?? ""`) so the
    /// typed value is byte-identical to the JSON path's `null`. `children` is
    /// a FlatBuffers vector of strings — absent decodes to `[]` (matching the
    /// Rust `Vec<String>` default).
    static func discoveredGroups(
        _ reader: nmp_nip29_DiscoveredGroupsSnapshot
    ) -> DiscoveredGroupsSnapshot {
        DiscoveredGroupsSnapshot(
            hostRelayUrl: reader.hostRelayUrl ?? "",
            groups: reader.groups.map { row in
                DiscoveredGroup(
                    groupId: row.groupId ?? "",
                    hostRelayUrl: row.hostRelayUrl ?? "",
                    name: row.name,
                    picture: row.picture,
                    about: row.about,
                    memberCount: row.memberCount,
                    adminCount: row.adminCount,
                    public: row.public_,
                    open: row.open_,
                    parent: row.parent,
                    children: row.children.map { $0 ?? "" }
                )
            }
        )
    }

    // MARK: app.29er.group_tree -> GroupTreeSnapshot

    static func groupTree(_ reader: nmp_app_29er_GroupTreeSnapshot) -> GroupTreeSnapshot {
        let nodes = reader.nodes.map(groupTreeNode(_:))
        return GroupTreeSnapshot(
            hostRelayUrl: reader.hostRelayUrl ?? "",
            roots: reader.roots.map(groupTreeNode(_:)),
            allNodes: Dictionary(uniqueKeysWithValues: nodes.map { ($0.groupId, $0) }),
            totalCount: Int(reader.totalCount)
        )
    }

    private static func groupTreeNode(_ row: nmp_app_29er_GroupTreeNode) -> GroupTreeNode {
        GroupTreeNode(
            groupId: row.groupId ?? "",
            hostRelayUrl: row.hostRelayUrl ?? "",
            name: row.name,
            parentId: row.parentId,
            childIds: row.childIds.map { $0 ?? "" },
            memberCount: row.memberCount,
            adminCount: row.adminCount,
            isPublic: row.public_,
            isOpen: row.open_,
            isMember: row.isMember,
            isAdmin: row.isAdmin,
            isBranch: row.branch,
            lastMessageId: row.lastMessageId,
            lastMessagePubkey: row.lastMessagePubkey,
            lastMessagePreview: row.lastMessagePreview,
            lastMessageCreatedAt: row.lastMessageCreatedAt,
            unreadCount: row.unreadCount
        )
    }

    // MARK: app.29er.relay_selector -> RelaySelectorSnapshot

    static func relaySelector(
        _ reader: nmp_app_29er_RelaySelectorSnapshot
    ) -> RelaySelectorSnapshot {
        RelaySelectorSnapshot(
            activeRelayUrl: reader.activeRelayUrl ?? "",
            relays: reader.relays.map { row in
                RelaySelectorRow(
                    relayUrl: row.relayUrl ?? "",
                    selected: row.selected,
                    fromNip51: row.fromNip51
                )
            }
        )
    }

    // MARK: relay_diagnostics → RelayDiagnosticsSnapshot

    static func relayDiagnostics(
        _ reader: nmp_kernel_RelayDiagnosticsSnapshot
    ) -> RelayDiagnosticsSnapshot {
        RelayDiagnosticsSnapshot(
            relays: reader.relays.map { row in
                let info = row.info
                return RelayDiagnosticsRelay(
                    relayUrl: row.relayUrl ?? "",
                    connection: row.connection ?? "",
                    nip11Name: info?.hasName == true ? info?.name : nil
                )
            }
        )
    }
}
