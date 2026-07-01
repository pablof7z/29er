# 29er-tui

A terminal UI for 29er — a NIP-29 relay-based group chat client.

## Running

```bash
# From the workspace root:
NMP_NSEC=nsec1... cargo run -p tui-29er --bin 29er-tui

# Without credentials (demo mode):
cargo run -p tui-29er --bin 29er-tui
```

The login relay field is pre-filled from `nmp-app-29er`'s Rust-owned app
configuration. Edit that field during login to use a different relay.

## Keybindings
| Key | Action |
|-----|--------|
| j / ↓ | Navigate down |
| k / ↑ | Navigate up |
| Enter | Open selected channel |
| Tab | Cycle focus (rooms → chat → composer) |
| n | Focus composer |
| / or Ctrl-K | Open command palette |
| Page Up/Down | Scroll messages |
| Esc | Back / close modal |
| q / Ctrl-C | Quit |

## Testing
```bash
cargo test -p tui-29er
```

## Terminal smoke test
```bash
# Verify the app launches, renders one frame, and exits cleanly on q:
tmux new-session -d -s smoke -x 180 -y 48 && \
  tmux send-keys -t smoke "cargo run -p tui-29er --bin 29er-tui" Enter && \
  sleep 3 && \
  tmux capture-pane -t smoke -p && \
  tmux send-keys -t smoke "q" && \
  tmux kill-session -t smoke
```

## Architecture — NMP ownership boundary

**What lives in NMP (Rust kernel, never in TUI):**
- Event signing and publishing
- Relay transport, connection management, retry policy
- Group membership and admin truth (from relay projections)
- Publish outbox state and retry eligibility
- NIP-29 event kind/tag construction

**What lives in the TUI (rendering + input only):**
- Screen layout and keyboard routing
- Snapshot rendering from NMP projections
- Typed action dispatch via `nmp_nip29::action::*`
- Local UI state (focus, scroll, palette open)

**The TUI NEVER:**
- Stores nsec after handing it to NMP
- Constructs Nostr events or tags directly
- Owns relay defaults or hidden relay fallbacks
- Derives membership/admin state locally
