# Conversations (fili-im)

Instant messaging for [Filigrane](https://github.com/filigrane-os): WhatsApp and Google Messages (SMS, MMS, RCS), then
Telegram, Signal, Matrix, Discord… in one light native application.

- **A daemon** (`fili-im daemon`) stays connected, keeps every message in a local SQLite database with full-text
  search, notifies new messages even with the window closed, and feeds the fili-bar's `messages` module.
- **Connectors**, one small program per platform (`fili-im-whatsapp`, …), speak a documented JSON-lines protocol
  with the daemon ([docs/connector-protocol.md](docs/connector-protocol.md)). Platform differences are declared
  capabilities, never special cases in the interface.
- **A window** (Qt Quick) and a **CLI** are clients of the daemon.

Status: 0.1.4, with the WhatsApp and Google Messages connectors. See [docs/plan.md](docs/plan.md).

| Version | Daemon | Connectors | Window |
|---|---|---|---|
| 0.4.2 | 27 MB RAM, 0 % CPU idle | WhatsApp 23 MB, Google Messages 19 MB | as 0.4.1 (one QML change) |
| 0.4.1 | 27 MB RAM, 0 % CPU idle | WhatsApp 23 MB, Google Messages 19 MB | 139 MB just after opening |
| 0.1.4 | 28 MB RAM, 0 % CPU idle | WhatsApp 25 MB, Google Messages 21 MB | not measured (Debug panel only adds a closed panel) |
| 0.1.0 | 30 MB RAM, 0.1 % CPU idle | WhatsApp 26 MB, Google Messages 19 MB | about 340 MB after 8 minutes of use (Qt Quick, pictures and previews), 2.7 % CPU averaged over that time |

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

## Google Messages

`connectors/gmessages/` is a Go program on [libgm](https://github.com/mautrix/gmessages) (AGPL-3.0, the "Messages for
web" protocol of the Android app): the phone's SMS, MMS and RCS conversations, linked like a browser. The phone must
stay on and online.

Google removed the QR code from recent versions of the phone app, so the default pairing is the Google account: a
browser window (Chrome, Chromium, Brave or Edge) opens on a throw-away profile, you sign in, the connector reads that
profile's session cookies, deletes the profile, and the phone asks you to tap an emoji. The cookies stay in the
account's private directory (0600) because Google Messages needs them to keep the session; removing the account
deletes them. The QR code stays available in the pairing dialog ("Use a QR Code Instead") for phones that still have the scanner. The protocol is not official:
Google can change it.

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
