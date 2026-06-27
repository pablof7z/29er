---
type: noun-entry
slug: content-string-responsibility
name: "Content string responsibility"
origin: extracted
source_refs:
  - transcript:1638-1639
---

# Content string responsibility

The content string (including mention text formatting like `nostr:npub1...`) is shell-owned. NMP's role with mentions is only to normalize `mention_pubkeys` into `["p", "<hex>"]` tags. Each shell (Swift and TUI) constructs its own mention text independently.
