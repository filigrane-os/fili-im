# Connector protocol, version 1

A connector links one account of one platform (WhatsApp, Telegram…) to the Conversations daemon. The daemon never
speaks a platform protocol; connectors never touch the store, the window, notifications or the bar.

This page is the contract. `src/protocol.rs` holds its Rust types, `fili-im-fake` implements it for tests and
interface work, and `tests/conformance.rs` checks any connector against it:

```
FILI_IM_CONNECTOR=/path/to/fili-im-whatsapp cargo test --test conformance
```

## Process

- Program name `fili-im-<platform>`, found next to `fili-im`, then in `FILI_IM_CONNECTOR_DIR`, then in `PATH`.
- Started by the daemon, one process per account:
  `fili-im-<platform> --account <key> --data <dir>`.
  `<key>` is the daemon's name for the account (`whatsapp-1`); `<dir>` is the account's private directory (0700),
  where the connector keeps its session and keys. Nothing else on disk belongs to the connector.
- stdin: commands from the daemon. stdout: events and replies. One JSON object per line, UTF-8, no pretty-printing.
- stderr: free-form log, saved by the daemon to `~/.local/state/fili-im/<key>.log`. Never write message contents,
  keys or tokens there.
- The connector exits on `shutdown`, on end of stdin, or on SIGTERM. The daemon restarts it with back-off when it
  exits unexpectedly (1 s, 2 s, 4 s… up to 60 s).

## Messages

Three shapes, told apart by their first key:

```json
{"event": "state", "state": "connected"}                    // connector → daemon, unsolicited
{"id": 7, "cmd": "send", "chat": "33600000001@s.whatsapp.net", "client_id": "c1f…", "text": {"plain": "Hi"}}
{"reply": 7, "ok": true, "result": {"id": "3EB0…", "timestamp": 1763200000000}}
{"reply": 8, "ok": false, "error": {"code": "unsupported", "message": "edit is not available"}}
```

- Every command gets exactly one reply with the same `id`, in any order (replies may interleave with events).
- Unknown keys are ignored on both sides (forward compatibility). An unknown command gets the error `unsupported`.
- Timestamps are milliseconds since the Unix epoch. Ids are opaque strings, unique within their scope (chat ids and
  contact ids within the account, message ids within the chat).

### Error codes

| Code | Meaning |
|---|---|
| `unsupported` | Unknown command or capability not available. |
| `invalid` | Bad parameters (missing chat, empty message…). |
| `not_found` | The chat, message or media does not exist (any more). |
| `not_connected` | Not connected; the daemon keeps the command (outbox) and retries after `connected`. |
| `rate_limited` | Slow down; `retry_after_s` may be given. |
| `failed` | Anything else; `message` says what, without private content. |

## Text

Text is neutral: plain text and spans. The connector converts from and to its platform's markup (WhatsApp `*bold*`,
Telegram entities, Discord Markdown).

```json
{"plain": "See you at Café Brun", "spans": [{"start": 11, "end": 20, "style": "bold"}]}
```

- `start`/`end` count Unicode scalar values (Rust `char`s), end excluded.
- `style`: `bold`, `italic`, `strike`, `code`, `pre`, `link` (with `url`), `mention` (with `contact`, a contact id).
- `spans` may be absent. A connector with `formatting: "plain"` ignores spans when sending.

## Hello and capabilities

The first line a connector writes is `hello`; the daemon sends nothing before it.

```json
{"event": "hello", "protocol": 1, "platform": "whatsapp", "name": "WhatsApp", "version": "0.1.0",
 "capabilities": {
   "pairing": ["qr", "code"],
   "message_kinds": ["text", "image", "video", "audio", "voice", "file", "sticker", "location", "contact"],
   "edit": {"window_s": 900},
   "delete": {"window_s": 172800},
   "reactions": "one_per_user",
   "replies": true,
   "threads": false,
   "receipts": {"send": true, "suppress": true},
   "typing": {"send": true, "receive": true},
   "history": "on_pairing",
   "schedule_native": false,
   "media_max_bytes": 104857600,
   "chat_kinds": ["direct", "group"],
   "formatting": "spans",
   "identity": {"phone": true},
   "presence": true,
   "avatars": true,
   "streaming": true
 }}
```

| Capability | Values | Absent means |
|---|---|---|
| `pairing` | list of `qr`, `code`, `token`, `oauth`, `password` | no pairing needed |
| `message_kinds` | kinds the connector can **send** (it may receive more) | `["text"]` |
| `edit`, `delete` | `{"window_s": N}` (N absent = no limit) | not available |
| `reactions` | `none`, `one_per_user`, `many` | `none` |
| `replies`, `threads` | quote replies; real threads | `false` |
| `receipts.send` | can mark messages read on the platform | `false` |
| `receipts.suppress` | reading never tells the sender unless `mark_read` is sent | `false` |
| `typing.send`, `typing.receive` | typing indicators | `false` |
| `history` | `none`, `on_pairing`, `on_demand` | `none` |
| `schedule_native` | the server holds scheduled messages | `false` |
| `media_max_bytes` | largest file it can send | no stated limit |
| `chat_kinds` | `direct`, `group`, `channel`, `space` | `["direct"]` |
| `formatting` | `plain`, `spans` | `plain` |
| `identity.phone` | contacts carry phone numbers (used to match people) | `false` |
| `presence` | the connector accepts `presence` (`state`: `available` shows the user online, `unavailable` not) | `false` |
| `streaming` | the connector accepts `stream` (`chat`, `message`, `ref`): `{url, mime}`, a local `http://127.0.0.1:…` address that serves the file with range requests, so a player can start and seek without a download | `false` |
| `avatars` | the connector accepts `avatar` (`chat`, `dest`: saves the small profile picture there, `not_found` when there is none or it is hidden) | `false` |

Capabilities are a ceiling. The daemon refuses a command whose capability is absent before sending it, and the
interface hides it. The daemon refuses a connector whose `protocol` it does not know.

## Events (connector → daemon)

### `state`

```json
{"event": "state", "state": "connected", "self": {"id": "33600000001@s.whatsapp.net", "name": "Alex", "phone": "+33600000001"}}
{"event": "state", "state": "phone_offline", "detail": "The phone has not been online for 9 days", "logout_in_s": 432000}
```

`state`: `connecting`, `connected`, `needs_pairing`, `logged_out`, `banned`, `phone_offline`, `error`. `self` comes with
`connected` once known. `detail` is shown to the user as is (English, the daemon may translate known states).

### `pairing`

```json
{"event": "pairing", "kind": "qr", "data": "2@Ab…", "expires_s": 20}
{"event": "pairing", "kind": "code", "data": "ABCD-EFGH", "expires_s": 180}
```

Sent after `pair`, renewed until paired, then `state connected`.

### `chat.upsert`

```json
{"event": "chat.upsert", "chat": {"id": "1203…@g.us", "kind": "group", "title": "Family", "unread": 3,
 "muted_until": 0, "archived": false, "pinned": false, "last_read": "3EB0…"}}
```

Only `id` is required; absent fields keep their stored value. `unread` is the platform's count; the daemon uses it
until it has seen the messages itself. `muted_until`: 0 not muted, -1 forever, else a timestamp.

### `contact.upsert`

```json
{"event": "contact.upsert", "contact": {"id": "33600000002@s.whatsapp.net", "name": "Paul", "phone": "+33600000002"}}
```

One stable id per person on the platform (WhatsApp maps phone-number ids and LIDs to one id before sending).

### `message.new` and `history.batch`

```json
{"event": "message.new", "live": true, "message": {
  "id": "3EB0…", "chat": "1203…@g.us", "sender": "33600000002@s.whatsapp.net", "from_me": false,
  "timestamp": 1763200000000, "kind": "text", "text": {"plain": "On mange où ?"},
  "reply_to": "3EA9…",
  "media": {"kind": "image", "mime": "image/jpeg", "name": "IMG_1.jpg", "size": 182311, "ref": "opaque"},
  "client_id": "c1f…"}}
{"event": "history.batch", "messages": [ … ], "progress": {"done": 400, "total": 2300}}
```

- `live: true`: just arrived; it may be notified. History (`history.batch`, or `live: false`) is never notified.
- `sender` is a contact id, absent when `from_me`. `kind` is one of the message kinds; unknown kinds are shown as
  "Unsupported message" with their `text` if any.
- `media.ref` is opaque, passed back to `fetch_media`.
- `client_id` is set when the connector sees its own sent message come back (echo), so the daemon matches it to the
  pending message.
- A message with an existing `id` updates the stored one.

### `message.edit`, `message.delete`, `reaction`, `receipt`, `typing`

```json
{"event": "message.edit", "chat": "…", "id": "…", "text": {"plain": "…"}, "timestamp": 1763200005000}
{"event": "message.delete", "chat": "…", "id": "…"}
{"event": "reaction", "chat": "…", "message": "…", "sender": "…", "from_me": false, "emoji": "👍", "timestamp": 1763200006000}
{"event": "receipt", "chat": "…", "messages": ["…"], "kind": "read", "sender": "…", "timestamp": 1763200007000}
{"event": "typing", "chat": "…", "sender": "…", "active": true}
```

An empty `emoji` removes the reaction. `receipt.kind`: `delivered`, `read` (`from_me` messages only, unless
`from_me: true` in the receipt: the user read on another device, so the chat becomes read here too).

### `error`

```json
{"event": "error", "message": "history sync failed, will retry"}
```

Non-fatal, logged and shown in the footer.

## Commands (daemon → connector)

| Command | Parameters | Result | Capability |
|---|---|---|---|
| `pair` | `method`, `phone` (for `code`) | `{}` then `pairing` events | `pairing` |
| `logout` | | `{}`; the connector forgets its session | |
| `send` | `chat`, `client_id`, `text` (the caption when there is a `media`), `reply_to`, `media` `{path, kind, mime, name}` (`kind`: `image`, `video` or `file`; the daemon copies the user's file and passes that copy) | `{id, timestamp}` | `message_kinds`, `replies` |
| `edit` | `chat`, `message`, `text` | `{}` | `edit` |
| `delete` | `chat`, `message`, `for_everyone` | `{}` | `delete` |
| `react` | `chat`, `message`, `emoji` (empty removes) | `{}` | `reactions` |
| `mark_read` | `chat`, `messages` (ids), `sender` (groups) | `{}` | `receipts.send` |
| `typing` | `chat`, `active` | `{}` | `typing.send` |
| `avatar` | `chat`, `dest` | `{}` (a picture saved at `dest`), or `not_found` | `avatars` |
| `presence` | `state` (`available`, `unavailable`) | `{}` | `presence` |
| `fetch_media` | `chat`, `message`, `ref`, `dest` (file path to write) | `{path}` | |
| `stream` | `chat`, `message`, `ref` | `{url, mime}` | `streaming` |
| `backfill` | `chat`, `before` (message id), `limit` | `{}` then `history.batch` | `history: on_demand` |
| `chat.mute` | `chat`, `until` (0, -1, timestamp) | `{}` | |
| `chat.archive` | `chat`, `archived` | `{}` | |
| `shutdown` | | `{}`, then exit | |

Parameters never use the names `id` and `cmd`, which belong to the envelope (a message is `message`).

`send` is idempotent on `client_id`: the daemon may send the same `client_id` again after a restart; a connector that
already sent it answers with the same `id`.

## Versioning

`protocol` changes only for incompatible changes. New events, commands, fields and capabilities are added within a
version: absent means unavailable, unknown is ignored.
