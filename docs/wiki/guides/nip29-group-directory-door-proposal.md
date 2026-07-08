# Proposal: a viewport-driven NIP-29 group-directory door in NMP

**Status:** proposal (revised after adversarial review) · **Motivated by:** [#60](https://github.com/pablof7z/29er/issues/60) (discovery emit-loop stall) · **Audience:** NMP + 29er

## Two distinct problems — don't conflate them

An adversarial review of the first draft of this proposal established that #60 has **two** causes at different layers, and an earlier version of this doc hid the first behind the second:

1. **An NMP kernel defect (the trap).** NMP runs host snapshot-projection closures *while holding the registry lock*, and opening/closing a read-session from inside such a closure re-locks the same lock → same-thread deadlock. This is a general footgun any compositional app will hit, tracked separately at **[nostr-multi-platform#3078](https://github.com/pablof7z/nostr-multi-platform/issues/3078)**. This proposal does **not** fix it and must not be treated as fixing it — #3078 is the real fix.
2. **A 29er design smell (the screen).** 29er hand-rolls per-row fan-out (a `Mutex<BTreeMap>` reconciler) and happens to run it inside the snapshot closure, which is what *trips* the trap. This proposal addresses the screen: move the fan-out into NMP so no app hand-rolls it.

Fixing only #3078 makes the freeze impossible but leaves every app re-implementing the fan-out; fixing only this proposal removes 29er's fan-out but leaves the trap armed for the next app. Both are needed.

## TL;DR

Listing groups is a one-liner (`open_nip29_group_discovery_session_with_reader` — what `shakeout` calls). Rendering a *discover screen* — each row carrying last-message preview, unread count, typing, and viewer membership — is not, because that per-row data lives in five other single-purpose doors that must be fanned out one-per-group and kept in sync as the group list changes.

Today 29er does that fan-out itself, in `crates/nmp-app-29er/src/group_sessions.rs` + `group_preview.rs` + `group_presence.rs`: a hand-written desired/stale/GC reconciler over a `Mutex<BTreeMap<group_id, session>>`, driven **eagerly** (every group, always) from **inside the typed-snapshot closure**. Because that closure opens/closes read-sessions, it trips the kernel deadlock (#3078) and stalls the emit loop. That is issue #60.

This proposes moving the fan-out into an NMP-owned **group-directory door** that:

1. owns the per-group sub-session fan-out internally (no app-side reconciler);
2. is **viewport-driven** — the shell reports which group ids are visible, and the door opens full preview/presence only for the visible window (plus a cheap always-on rollup for sorting);
3. emits one reader / one keyed frame, exactly like `shakeout`'s one-liner.

This removes the stall (fan-out becomes event-driven off viewport + discovery changes, never run inside the snapshot fold), removes the eagerness (no 500 sessions for 500 groups), removes 29er's bespoke reconciler and its hand-maintained `N29T`/`NDGS` composite schema, and generalizes to any "list of things, one live resource per thing" screen.

## Why the composite can't just move to the view layer

(Recorded here because it's the first thing everyone proposes.) "Let each SwiftUI row open its own kind:9" fails on one hard count:

- **The shell must not own fetch logic** (kinds, relay pinning, filter shape, dedup, "what is the last message"). Moving raw subscriptions into Swift reimplements business rules per shell (iOS, TUI, next). The row may report *intent* ("I'm visible"); it may not own the *fetch*.

> **Correction from review.** An earlier draft also argued the composite is "forced" because NMP delivers one whole-tree snapshot frame per tick. That was wrong: the frame is already *row-incremental* (a projection returning `None` means "retain", an unregister means "cleared row", per ADR-0072 incremental apply). Single-frame delivery was never the constraint — *where the fan-out reconcile runs* was. So delivery is sound; keep it as a given. The keyed shape is a natural fit, not a straitjacket.

The synthesis — row reports viewport intent, Rust owns the fan-out — is what NMP already does for feeds ("native shells only render the emitted projection and report viewport intent", `app_impl_feeds.rs:18`; "shells report viewport intent by key. The controller and page policy live in NMP", `feed.rs:4`). This proposal applies that same shape to group discovery.

## The primitive to mirror: `register_feed`

NMP feeds already express "app registers one controller under a key; NMP owns viewport, paging, and per-tick projection":

```rust
// nmp-native-runtime/src/app_impl_feeds.rs:19
pub fn register_feed(&self, key: impl Into<String>, controller: Arc<dyn nmp_feed::FeedController>)
```

The group directory is the same idea with a fan-out controller instead of a paging controller.

## Where the fan-out belongs: generalize `nmp-read-session`, don't hand-build it in `nmp-nip29`

**Correction from review.** The first draft implied `Nip29GroupDirectorySession` would *contain* the fan-out — its own `Mutex<BTreeMap<group_id, sub-session>>` inside `nmp-nip29`. That is just 29er's reconciler moved down one crate, and it violates NMP's own doctrine that a concept crate "must not implement lifecycle mechanics" — those live once, in the engine.

NMP already has the embryo of the right primitive: `nmp-read-session`'s **dependent-demand reconciler** (`dependent.rs`), which derives sub-interest demand from admitted events and reconciles it on the non-blocking event path — i.e. "a collection drives a set of sub-interests," exactly this shape. Today it is limited to a *single* demand per provider, with no keyed set and no viewport gate.

So the real NMP work is to **generalize that reconciler to a keyed set of dependent demands, with an optional viewport filter over the key set** (open sub-sessions only for visible keys). Then:

- `nmp-read-session` owns the keyed fan-out + viewport gating + lifecycle (one place, engine-level, on the event path);
- `nmp-nip29`'s group-directory door becomes a **thin declarative spec** over that primitive — "for each discovered group, this is the preview/presence/membership demand" — and holds no reconciler of its own;
- other domains (feed author profiles, per-thread reactions, …) reuse the same primitive instead of re-hand-rolling.

This is the substance of the 29er follow-up [#61](https://github.com/pablof7z/29er/issues/61). The door API below is the *surface*; its implementation must sit on the generalized `nmp-read-session` primitive, not a bespoke map.

## Proposed API

### Door (NMP, `nmp-nip29`)

```rust
/// One relay's NIP-29 group directory, with per-group rollups fanned out
/// internally and gated by viewport intent. Replaces the app-side
/// discovery + preview + presence + joined composition.
pub struct Nip29GroupDirectorySession {
    host_relay_url: RelayUrl,
    /// Viewer whose membership + read-state the rollups are computed for.
    viewer: PublicKey,
    /// What each visible row needs. The door owns the filter shapes,
    /// kinds, relay-pinning, and dedup — the shell never sees them.
    rollups: GroupRollupSpec, // { preview: bool, unread: bool, typing: bool, membership: bool }
}

pub fn open_nip29_group_directory_session_with_reader(
    app: &NmpApp,
    session: Nip29GroupDirectorySession,
) -> (Nip29GroupDirectoryHandle, Arc<GroupDirectoryProjection>);

pub fn close_nip29_group_directory_session(app: &NmpApp, handle: Nip29GroupDirectoryHandle);
```

### Viewport intent (NMP native runtime — new, mirrors feed viewport)

```rust
/// Report which group ids are currently visible for a directory session.
/// The door opens full preview/presence sub-sessions for `visible` only,
/// closes them for ids that dropped out, and keeps a cheap always-on
/// last-activity + unread rollup for ALL groups so the shell can sort/badge
/// the whole set without holding every preview session live.
///
/// Non-blocking: records intent and marks the kernel dirty; the actual
/// open/close reconcile runs on the actor's own event path, never inside a
/// snapshot fold.
pub fn report_group_directory_viewport(
    &self,
    handle: &Nip29GroupDirectoryHandle,
    visible: &[GroupId],
);
```

### Reader shape (NMP)

`GroupDirectoryProjection::snapshot()` returns one keyed structure the shell renders directly — no app-owned FlatBuffers composite:

```
rows: [ GroupDirectoryRow {
          group_id, name, picture, about,
          last_activity_at,          // always present (cheap rollup) → enables sort
          unread,                    // always present (cheap rollup) → enables badge
          preview: Option<Message>,  // present only while visible
          typing:  Vec<PublicKey>,   // present only while visible
          membership: Membership,    // is_member / is_admin, viewer-scoped
        } ]
```

### 29er UniFFI facade (this repo) collapses to

```rust
pub fn open_group_directory(&self, host_relay_url: String) -> bool // opens the ONE door
pub fn set_visible_groups(&self, group_ids_json: String)           // viewport intent
pub fn close_group_directory(&self)
```

`group_sessions.rs` (~250 lines), `group_preview.rs`, `group_presence.rs`, the `sync_joined_session` identity-observer dance, and the `N29T`/`NDGS` hand-maintained schema (source of the v2→v3 drift in #60 Finding 1) all delete. The Swift side reports visible rows via SwiftUI `.onAppear`/`.onDisappear` → `set_visible_groups`.

## How this relates to #60 (but is not the fix for the trap)

The stall is triggered by session open/close running inside the snapshot closure on the actor thread. Two things make it go away, at different layers:

- **The kernel fix ([#3078](https://github.com/pablof7z/nostr-multi-platform/issues/3078)) removes the trap** — once NMP stops holding the registry lock across app closures, opening a read-session mid-tick can no longer deadlock, for *any* app. This is the real fix and is independent of this proposal.
- **This proposal removes the thing that trips it** for the group screen: the fold becomes **pure** (reads `GroupDirectoryProjection`, encodes), and the fan-out reconcile runs on the **event path** via the generalized `nmp-read-session` primitive, triggered by a discovered-groups change or a `report_group_directory_viewport` call.

Both are worth doing: #3078 so the next app doesn't fall in, this proposal so 29er stops hand-rolling the fan-out at all.

## Rollout

1. **In-repo unblock first (small, ships now):** split `sync` (mutates interests) out of the snapshot closure in `group_sessions.rs:362` — keep the current eager composite, but drive `preview.sync`/`presence.sync` from an observer on the discovered-groups reader instead of from the fold. Fold becomes pure. This clears the stall on today's NMP without waiting for #3078 or the door. (Tracked on #60.)
2. **Kernel fix ([#3078](https://github.com/pablof7z/nostr-multi-platform/issues/3078)):** NMP releases the registry lock before running app closures. Removes the deadlock class entirely.
3. **Generalize `nmp-read-session`** to keyed dependent demands + viewport gating (the engine-level primitive; substance of #61).
4. **NMP door (this proposal):** land `Nip29GroupDirectorySession` as a declarative spec over the primitive; 29er swaps its composite for the three-call facade above.
5. **Delete** 29er's reconciler + `N29T`/`NDGS` schema once the door ships.

## Open questions

- Cheap always-on rollup: can `last_activity_at` + `unread` for *all* groups be served from discovery metadata / a lightweight aggregate without a full kind:9 session per group? (Needed so whole-set sort/badge doesn't reintroduce eager fan-out.)
- Should viewport intent debounce inside NMP (fast scroll = churn) or should the shell? Feeds' paging controller already debounces — reuse that.
- Membership is viewer-scoped and reactive across account switch (today's identity-change observer). The door owns `viewer`; account switch = re-open or a `set_viewer` call.
