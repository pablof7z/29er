---
type: episode-card
date: 2026-06-27
session: 2d0bcf1a-c9a6-4929-99bd-7956ebfabf16
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-29er/2d0bcf1a-c9a6-4929-99bd-7956ebfabf16.jsonl
salience: architecture
status: active
subjects:
  - nmp-api-migration
  - group-events-projection
  - event-registration-pattern
supersedes: []
related_claims: []
source_lines:
  - 557-584
captured_at: 2026-06-27T18:09:29Z
---

# Episode: Upgrade to NMP v0.8.4 event registration API (kind-specific groups, open_group_discovery)

## Prior State

29er's Slack-style chat renderer feature was written against NMP v0.8.2/v0.8.3: GroupTimelineProjection/GroupTimelineEvent types, open_group_timeline() for group chat access, and KernelEventObserver/register_live_event_tap for registration. The app consumed a monolithic pre-packaged timeline without declaring specific event kinds.

## Trigger

Merging main revealed the pinned NMP revision (aabe786) was v0.8.4 with breaking API changes. Merge conflicts in app.rs and ffi.rs exposed incompatible call sites; compilation would fail until migrated.

## Decision

Performed comprehensive migration to NMP v0.8.4 API surface: GroupTimelineProjection→GroupEventsProjection, GroupTimelineEvent→GroupEvent, open_group_timeline()→open_group_events(group_id, [kinds]), register_live_event_tap→open_observed_projection/open_group_discovery_with_reader, snapshot().messages→snapshot().events. Added type aliases for removed NMP types (EventClaimSink).

## Consequences

- Event consumption now consumer-declared: app explicitly requests kind filters (KIND_CHAT_MESSAGE, KIND_DISCUSSION_OR_ARTIFACT) per door instead of receiving monolithic timeline
- App struct extended with new fields (discovery_handle, group_tree_observer_id) for explicit handle lifecycle management
- Event accessor pattern changed from collection-based to instance-based (snapshot().events vs messages)
- Type imports and aliases updated across TUI (app.rs, chat.rs, tests.rs), iOS bridge, and components layer
- Components layer must provide local polyfills for NMP-removed types (EventClaimSinkProtocol)

## Open Tail

- EmbedHostEnvironment.swift:30,34 contain unnecessary nonisolated(unsafe) on Sendable constants (compiler warnings)
- NostrContentView.swift uses deprecated Text + Text concatenation (7 instances); iOS 26+ deployment target requires string interpolation

## Evidence

- transcript lines 557-584

## Conversation

- Cleaned transcript (verbatim user words, abbreviated agent replies): [`transcripts/2026-06-27-1-upgrade-to-nmp-v0-8-4.json`](transcripts/2026-06-27-1-upgrade-to-nmp-v0-8-4.json)
- Raw transcript (verbatim user words, full agent replies): [`transcripts/raw/2026-06-27-1-upgrade-to-nmp-v0-8-4.json`](transcripts/raw/2026-06-27-1-upgrade-to-nmp-v0-8-4.json)
