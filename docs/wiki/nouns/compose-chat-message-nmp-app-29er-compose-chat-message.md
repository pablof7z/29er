---
type: noun-entry
slug: compose-chat-message-nmp-app-29er-compose-chat-message
name: "compose_chat_message (nmp-app-29er::compose_chat_message)"
origin: extracted
source_refs:
  - transcript:2214-2220
  - transcript:2650-2657
---

# compose_chat_message (nmp-app-29er::compose_chat_message)

A shared Rust function that transforms raw text + selected mention pubkeys into NIP-21 format (replacing @token with nostr:npub1...) and builds p-tags — the single authoritative home for mention composition, called by both TUI and Swift before dispatching to publish_group_event
