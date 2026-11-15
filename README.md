# Conversations (fili-im)

Instant messaging for [Filigrane](https://github.com/filigrane-os): WhatsApp first, then Telegram, Signal, Matrix,
Discord… in one light native application.

- **A daemon** (`fili-im daemon`) stays connected, keeps every message in a local SQLite database with full-text
  search, notifies new messages even with the window closed, and feeds the fili-bar's `messages` module.
- **Connectors**, one small program per platform (`fili-im-whatsapp`, …), speak a documented JSON-lines protocol
  with the daemon ([docs/connector-protocol.md](docs/connector-protocol.md)). Platform differences are declared
  capabilities, never special cases in the interface.
- **A window** (Qt Quick, coming in phase P2) and a **CLI** are clients of the daemon.

Status: phase P0 (foundations) done: protocol, store, daemon, CLI, fake connector, conformance suite. See
[docs/plan.md](docs/plan.md).

## Try it with the fake connector

```sh
cargo build
target/debug/fili-im daemon &          # finds fili-im-fake next to it
target/debug/fili-im add fake          # → fake-1
target/debug/fili-im pair fake-1       # the fake pairs itself after a moment
target/debug/fili-im chats
target/debug/fili-im send 1 "Hello"    # Paul answers "Echo: Hello"
target/debug/fili-im search hello
target/debug/fili-im watch             # the daemon's events, as JSON lines
```

`--json` on any command prints JSON (for scripts and the AI agent).

## Files

| Path | What |
|---|---|
| `~/.local/share/fili-im/conversations.db` | the store (0600) |
| `~/.local/share/fili-im/accounts/<key>/` | a connector's session and keys (0700) |
| `~/.local/state/fili-im/<key>.log` | a connector's log (no message contents) |
| `~/.local/state/alerts/messages`, `messages.count` | what the fili-bar shows |
| `$XDG_RUNTIME_DIR/fili-im/daemon.sock` | the control socket |

## Development

`make check` (rustfmt, clippy, tests). The conformance suite runs against any connector:
`FILI_IM_CONNECTOR=/path/to/fili-im-<platform> cargo test --test conformance`.

License: GPL-3.0-or-later.
