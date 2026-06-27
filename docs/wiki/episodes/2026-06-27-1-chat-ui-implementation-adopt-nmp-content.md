---
type: episode-card
date: 2026-06-27
session: 9282ab25-3d0b-4e36-84e7-81006fdf1525
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-29er/9282ab25-3d0b-4e36-84e7-81006fdf1525.jsonl
salience: architecture
status: active
subjects:
  - chat-ui
  - nmp-content-renderer
  - component-registry
  - entity-resolution
supersedes: []
related_claims: []
source_lines:
  - 1-7
  - 95-203
  - 332-429
captured_at: 2026-06-27T05:24:38Z
---

# Episode: Chat UI implementation — adopt NMP content renderer and registry components

## Prior State

TUI chat renders raw content with no embed/entity support. iOS has no chat view. Implementation approach for 'proper rendering of nostr entities' is unspecified; implicit risk of hand-parsing content in shell.

## Trigger

User requirement for Slack-style chat UX with embed support and explicit instruction to do this 'the RIGHT way -- no hacks'; research revealed NMP content tokenizer and complete platform-specific rendering component stacks already exist in workspace registry.

## Decision

Adopt NMP content tokenizer (nmp-content::tokenize) as single source for all shell content rendering. Distribute platform-specific components (TUI: NostrContentView widget; iOS: SwiftUI NostrContentView) via registry using 'nmp add component' to keep app-owned/updatable. Separate Slack layout (shell presentation) from content rendering (kernel substrate via FFI). Route entity/embed resolution through existing kernel seam (nmp_app_resolve_ref).

## Consequences

- Shell code eliminates hand-parsing risk; content tokenization stays in kernel substrate
- Components become app-owned via registry, updatable via 'nmp update component' instead of bespoke or copy-paste
- Both TUI and iOS achieve feature parity with consistent rendering (mentions, embeds, media, markdown)
- Entity resolution is reactive: shell claims → kernel fetches → snapshot carries envelope → shell re-renders
- Slack presentation (author grouping, avatar gutter, day dividers) cleanly separated from tokenization logic

## Open Tail

- Implementation sequence: user decision pending on phase rollout (TUI+Phase-0 first vs. both shells in parallel)

## Evidence

- transcript lines 1-7
- transcript lines 95-203
- transcript lines 332-429

## Conversation

- Cleaned transcript (verbatim user words, abbreviated agent replies): [`transcripts/2026-06-27-1-chat-ui-implementation-adopt-nmp-content.json`](transcripts/2026-06-27-1-chat-ui-implementation-adopt-nmp-content.json)
- Raw transcript (verbatim user words, full agent replies): [`transcripts/raw/2026-06-27-1-chat-ui-implementation-adopt-nmp-content.json`](transcripts/raw/2026-06-27-1-chat-ui-implementation-adopt-nmp-content.json)
