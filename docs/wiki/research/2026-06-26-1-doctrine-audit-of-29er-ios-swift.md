---
type: research-record
date: 2026-06-26
session: 00700b1c-4fd7-4a41-90e9-7a76d64b42cc
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-29er/00700b1c-4fd7-4a41-90e9-7a76d64b42cc.jsonl
source_lines: 2705-2756
agent_attribution: main
has_preregistered_criteria: true
has_method: true
has_structured_report: true
characterization: "Doctrine audit of 29er iOS Swift: seven parallel finders across predefined dimensions; 18 raw findings filtered to 8 confirmed violations via adversarial verification against NMP API and Chirp reference"
captured_at: 2026-06-26T11:07:29Z
---

Doctrine audit of 29er iOS Swift: seven parallel finders across predefined dimensions; 18 raw findings filtered to 8 confirmed violations via adversarial verification against NMP API and Chirp reference

---

User: [Tool result]: Workflow launched in background. Task ID: w6gujdsf0
Summary: Audit 29er iOS Swift for NMP doctrine violations (business logic that belongs in Rust/NMP)
Transcript dir: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-29er/00700b1c-4fd7-4a41-90e9-7a76d64b42cc/subagents/workflows/wf_2406a47a-0c3
Script file: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-29er/00700b1c-4fd7-4a41-90e9-7a76d64b42cc/workflows/scripts/swift-doctrine-audit-wf_2406a47a-0c3.js
(Edit this file with Write/Edit and re-invoke Workflow with {scriptPath: "/Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-29er/00700b1c-4fd7-4a41-90e9-7a76d64b42cc/workflows/scripts/swift-doctrine-audit-wf_2406a47a-0c3.js"} to iterate without resending the script.)
Run ID: wf_2406a47a-0c3
To resume after editing the script: Workflow({scriptPath: "/Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-29er/00700b1c-4fd7-4a41-90e9-7a76d64b42cc/workflows/scripts/swift-doctrine-a

Assistant: Audit running (`w6gujdsf0`). Seven parallel finders, one per doctrine dimension, each comparing 29er's Swift against both the NMP API and the **Chirp iOS shell** (the doctrine-correct reference):

1. **Events/tags/kinds** — hand-built Nostr events, kind ints, tag arrays
2. **NIP-19/21** — bech32 encode/decode, `nostr:` URI construction (the mention-bug archetype)
3. **Membership/admin** — locally-derived admin truth or permission gating
4. **Relay policy** — hardcoded relays/defaults/fallback
5. **Content parsing** — links/hashtags/mentions tokenized in Swift instead of `nmp_content`
6. **Retry/outbox** — confirmation/retry policy in Swift
7. **Projection derivation** — unread counts, tree building, sort/dedup rules NMP already emits

Each raw finding then gets **adversarially verified** (default-to-false, confirm only with a real NMP replacement) to filter out allowed display formatting like `short_npub` truncation. You'll get a ranked report of confirmed violations with concrete fixes and a fix order.

User: [Agent task result: Dynamic workflow "Audit 29er iOS Swift for NMP doctrine violations (business logic that belongs in Rust/NMP)" completed]
{"status":"audit-complete","rawFindings":18,"confirmedViolations":8,"report":"# NMP Doctrine Violations — 29er iOS Swift Shell\n\n## HIGH\n\n### 1. Outbox→group scoping via raw `[\"h\", groupId]` tag + magic kind:9\n`ios/29er/29er/Features/GroupTreeView.swift:235-245`\n**Wrong:** `outboxMessages` filters the kernel publish-outbox by hardcoded `item.kind == 9` and walks raw tag arrays (`tag[0] == \"h\" && tag[1] == groupId`) to decide which in-flight events belong to this group — kernel-owned event-membership truth (`GroupChatProjection.accepts()` in `nmp-nip29/src/projection/group_chat.rs`). 29er even hand-added a non-canonical `tags: [[String]]` field to `PublishOutboxItem` to enable this; canonical NMP/Chirp deliberately omit it.\n**Fix:** Emit pending/optimistic group messages inside `GroupChatProjection` (or a per-group outbox projection keyed by `GroupId`). Remove `kind`/`tags` from `PublishOutboxItem` (`Bridge/KernelSnapshotTypes.swift:97-99`) as Chirp did. Shell `ForEach`s one ready list.\n\n### 2. Outbox/chat projection join + dedup business rules in the shell\n`ios/29er/29er/Features/GroupTreeView.swift:235-245` (same property as #1)\n**Wrong:** Joins `group_chat` (NGCS) + `publish_outbox` (KPBO), dedups outbox against delivered ids (`!deliveredIds.contains(item.eventId)`), and re-orders — outbox-confirmation/dedup policy reserved for the kernel.\n**Fix:** `nmp-app-29er` group_chat projection should emit a single ordered list including optimistic sends with per-message delivery status (as Chirp's `GroupChatProjection` does / Chirp simply waits for the snapshot tick). Reconciliation belongs in Rust.\n\n### 3. Viewer membership/admin truth derived by roster scan\n`ios/29er/29er/Features/GroupTreeView.swift:259-279`\n**Wrong:** `isCurrentMember`/`isCurrentAdmin` computed by matching `activeAccountPubkey` against the member roster and reading per-row `.admin`. This is viewer admin/membership truth the kernel owns — and the roster path is dead (`select_group_members` is a documented no-op per `nmp-app-29er/src/ffi.rs:480-497`), so both flags are permanently false.\n**Fix:** Read kernel-emitted viewer truth from `JoinedGroupsProjection` (`nmp-nip29/schema/joined_groups.fbs` → per-group `is_member`/`is_admin` keyed on `active_pubkey`). Surface these onto `GroupTreeNode` (`nmp-app-29er/schema/group_tree.fbs`) and read `node.isMember`/`node.isAdmin`.\n\n## MEDIUM\n\n### 4. Admin/composer UI gated on locally-computed status\n`ios/29er/29er/Features/GroupTreeView.swift:255-256, 563-581`\n**Wrong:** `canCompose` and the entire `AdminActionsSheet` (createInvite/putUser/createChild/setParent = kinds 9009/9000/9007/9002) are gated on the roster-derived `isCurrentMember`/`isCurrentAdmin` — the shell deciding which membership/admin actions are permitted.\n**Fix:** Gate on the projection-emitted `is_admin`/`is_member` from #3. Shell may still dispatch typed admin actions; it must not decide eligibility. (Relay still enforces real authorization, hence medium.)\n\n### 5. Manual @mention tokenization + NIP-19 identifier detection\n`ios/29er/29er/Features/GroupTreeView.swift:854-879`\n**Wrong:** `rawMentionIdentifiers`/`looksLikeRawMentionIdentifier` tokenize draft content and classify tokens as Nostr identifiers via the `npub1` bech32 prefix and 64-hex pubkey shape — content parsing + NIP-19 recognition the shell must not own. This is the mention-bug archetype.\n**Fix:** Send only raw draft text + user-picked `selectedMentionPubkeys` (legit view state from `acceptMention`) to `nmp.nip29.post_chat_message`. Delete both helpers. Matches the sibling TUI (`29er-tui/src/ui/composer.rs`) and Chirp (`GroupChatView.sendDraft` → raw text only).\n\n### 6. Default NIP-29 relay URL + role hardcoded in Swift\n`ios/29er/29er/Bridge/KernelModel+Lifecycle.swift:7-8, 44`\n**Wrong:** Shell picks the default relay (`?? \"wss://nip29.f7z.io\"`), an env-override fallback policy (`M001_DEFAULT_RELAY_URL`), and the role (`\"both\"`), then seeds via `kernel.addRelay`. Relay default/fallback policy in the shell.\n**Fix:** Move URL+role into a Rust 29er config (analogous to `nmp_chirp_config::chirp_default_relay_bootstrap`), expose a seed FFI symbol (like `nmp_app_chirp_seed_default_relays`), and have Swift call `kernel.seedDefaultRelays()` with only the `NMP_TEST_RELAYS` env seam. No URL/role literal in Swift.\n\n### 7. New-group host relay pinned to Swift constant\n`ios/29er/29er/Bridge/GroupDiscoveryBridge.swift:333`\n**Wrong:** `GroupId(hostRelayUrl: defaultNip29RelayUrl, ...)` pins a created group's host relay to the hardcoded constant — a relay-host choice.\n**Fix:** Switch 29er Rust composition (`nmp-app-29er/src/ffi.rs:118`) from `wire_group_defaults` to `wire_group_defaults_with_relay(...)`, then have Swift read `model.groupDefaults.suggestedRelayUrl` (projection `nmp.nip29.group_defaults`) into `createPublicGroup` — exactly Chirp's `NewGroupSheet` pattern.\n\n### 8. Discovery screen opens against hardcoded relay constant\n`ios/29er/29er/Features/GroupTreeView.swift:54`\n**Wrong:** `model.openGroupDiscovery(hostRelayUrl: defaultNip29RelayUrl)` chooses the catalog-query relay via a Swift literal.\n**Fix:** Same as #7 — drive from the kernel-projected `groupDefaults.suggestedRelayUrl` (populated via `wire_group_defaults_with_relay`) or from user input.\n\n## Cross-Cutting Themes\n- **Two root archetypes.** (a) *Raw-tag/magic-kind protocol parsing in the shell* (#1, #2, #5) — the shell tokenizes content/tags and branches on kind ints NMP already owns; #1 even widened the wire DTO with a non-canonical `tags` field to enable it. (b) *Kernel-owned policy/truth re-derived in Swift* (#3, #4, #6, #7, #8) — viewer admin/membership and relay selection hardcoded or scanned instead of read from a projection.\n- **Dead code reinforces the diagnosis.** The roster path (#3/#4) is a documented no-op and `item.tags` is empty at the pinned kernel rev (#1), so these violations are also inverted/dead — protocol logic stranded in the shell, not live affordances.\n- **Chirp is the consistent counter-reference**: no roster scan, no outbox-join, no mention tokenization, no relay literals — relay defaults and optimistic merges live in Rust.\n\n## Recommended Fix Order\n1. **#3 + #4 together** — wire `JoinedGroupsProjection` `is_member`/`is_admin` onto `GroupTreeNode`; replace roster scan and re-gate UI. Highest sensitivity (admin/permission truth) and unblocks currently-broken composing.\n2. **#1 + #2 together** — emit optimistic/delivery-status messages in the Rust `group_chat` projection; render one list; then delete `kind`/`tags` from `PublishOutboxItem`. Removes the raw-tag foothold (also kills #5's sibling patterns).\n3. **#5** — drop `rawMentionIdentifiers`/`looksLikeRawMentionIdentifier`; send raw text + `selectedMentionPubkeys`. Small, self-contained, closes the mention archetype.\n4. **#6 → #7 → #8** — land the Rust 29er relay config + `seedDefaultRelays` FFI and `wire_group_defaults_with_relay`, then remove all three Swift relay literals in one pass (they share `defaultNip29RelayUrl`)."}

Assistant: Yes — the mention bug wasn't isolated. The audit found **8 confirmed violations** (filtered from 18 raw findings via adversarial verification against the NMP API + the Chirp reference shell). They cluster into the same two archetypes as the mention bug.

## High severity

**1 & 2 — Outbox/chat logic in the shell** (`GroupTreeView.swift:235-245`)
The shell filters the publish-outbox by hardcoded `kind == 9`, walks raw `["h", groupId]` tag arrays to decide which in-flight events belong to a group, then joins `group_chat` + `publish_outbox`, dedups against delivered IDs, and reorders. That's kernel-owned event-membership + confirmation/dedup policy. 29er even added a **non-canonical `tags` field** to `PublishOutboxItem` to enable this — Chirp deliberately omits it.
→ Rust `group_chat` projection should emit one ordered list including optimistic sends with delivery status.

**3 — Admin/membership truth derived by roster scan** (`GroupTreeView.swift:259-279`)
`isCurrentMember`/`isCurrentAdmin` computed by scanning the member roster against the active pubkey. The kernel owns this — and worse, the roster path is a **documented no-op** (`ffi.rs:480-497`), so both flags are permanently false.
→ Read `is_member`/`is_admin` from `JoinedGroupsProjection`, surfaced onto `GroupTreeNode`.

## Medium

**4** — Admin UI + composer gated on the (broken) locally-computed status above → gate on the projection fields from #3.
**5** — Manual `@mention` tokenization + `npub1`/64-hex identifier detection (`GroupTreeView.swift:854-879`) — the mention archetype again → send raw text + picked pubkeys, delete the helpers.
**6** — Default relay URL + role hardcoded in Swift (`KernelModel+Lifecycle.swift:7-8,44`) → move to Rust 29er config + a `seedDefaultRelays` FFI.
**7 & 8** — New-group host relay + discovery relay pinned to a Swift constant → drive from `groupDefaults.suggestedRelayUrl` (Chirp's pattern).

## The theme
Two roots: **(a)** shell parses raw tags / branches on magic kind ints NMP already owns (#1,2,5); **(b)** kernel-owned truth & relay policy re-derived in Swift (#3,4,6,7,8). Notably, the dead-code findings (#3 roster is a no-op, #1's `tags` is empty at the pinned rev) confirm these are stranded protocol logic, not working features.

## Fix order
1. **#3+#4** — wire `is_member`/`is_admin` onto `GroupTreeNode`; unblocks currently-broken composing (admin truth — highest sensitivity)
2. **#1+#2** — emit optimistic/delivery status in the Rust `group_chat` projection; render one list; delete `kind`/`tags` from `PublishOutboxItem`
3. **#5** — drop the mention tokenizers (small, self-contained)
4. **#6→#7→#8** — Rust relay config + `seedDefaultRelays` + `wire_group_defaults_with_relay`, then strip all three relay literals

Note several of these need **Rust/NMP-side work** (new projection fields, FFI symbols), not just Swift edits — that's the point: the logic moves *out* of Swift *into* the shared core.
