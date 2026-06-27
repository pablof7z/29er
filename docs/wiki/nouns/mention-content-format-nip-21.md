---
type: noun-entry
slug: mention-content-format-nip-21
name: "Mention content format (NIP-21)"
origin: extracted
source_refs:
  - transcript:1390-1391
---

# Mention content format (NIP-21)

When a user selects a mention in the composer, the inserted text in the message content must be `nostr:npub1...` (bech32-encoded public key), not `@display_name`. The mention pubkey is separately added as a NIP-29 `p`-tag.
