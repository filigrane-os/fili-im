# Conversations (fili-im)

Instant messaging for [Filigrane](https://github.com/filigrane-os): WhatsApp first, then Telegram, Signal, Matrix,
Discord… in one light native application.

- **A daemon** (`fili-im daemon`) stays connected, keeps every message in a local SQLite database with full-text
  search, notifies new messages even with the window closed, and feeds the fili-bar's `messages` module.
- **Connectors**, one small program per platform (`fili-im-whatsapp`, …), speak a documented JSON-lines protocol
  with the daemon ([docs/connector-protocol.md](docs/connector-protocol.md)). Platform differences are declared
  capabilities, never special cases in the interface.
- **A window** (Qt Quick, coming in phase P2) and a **CLI** are clients of the daemon.

Status: P0 (foundations) done; P1 (WhatsApp connector, text) built, waiting for a live test. See
[docs/plan.md](docs/plan.md).

## WhatsApp

`connectors/whatsapp/` is a Go program on [whatsmeow](https://github.com/tulir/whatsmeow) (MPL-2.0), linked to the
phone like WhatsApp Web. Building it needs Go and a C compiler (SQLite through cgo); `make build` puts
`fili-im-whatsapp` next to `fili-im`.

```sh
make release
target/release/fili-im daemon &
target/release/fili-im add whatsapp                       # → whatsapp-1
target/release/fili-im pair whatsapp-1                    # QR code in the terminal
target/release/fili-im pair whatsapp-1 --phone +33612345678   # or a code to type on the phone
```

WhatsApp does not allow third-party clients in its terms: using one on a personal account carries a small risk of a
ban. The phone stays the main device; a linked device left offline for about two weeks is logged out.

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

`make check` (rustfmt, clippy, gofmt, go vet, all tests, conformance of the WhatsApp connector). The conformance suite runs against any connector:
`FILI_IM_CONNECTOR=/path/to/fili-im-<platform> cargo test --test conformance`.

License: GPL-3.0-or-later.
