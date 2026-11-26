# Conversations (fili-im): plan

Status: P0 done 2026-11-15; P1 connector built 2026-11-15 (text, receipts, reactions, edits, deletes, history, media download; sending files left), waiting for a live test.

Conversations is Filigrane's instant messaging application: one place for WhatsApp, then Telegram, Discord, Signal and
others. Program `fili-im`, display name **Conversations** (plain in French and English, next to Casier, Facteur,
Aéroport, Cassette). `fili-chat` was taken by Lia, the AI chat (now `fili-ai`).

It follows Facteur's model: a **daemon** that stays connected and notifies even when the window is closed, a **fili-bar
module** with the unread count, and a **window** that is only a client of the daemon.

## 1. Goals

- Receive and answer messages from several platforms without a browser tab or an Electron client.
- Be notified with the window closed: the daemon is the application; the window can come and go.
- Do things the official clients do not (section 8): read without sending receipts, answer from the bar, one contact
  across platforms, a local searchable history the AI agent can use, snooze and scheduled send.
- Stay light: daemon under 30 MB RSS, each connector under 40 MB, nothing resident in Node, Electron or the JVM.

Non-goals: audio and video calls, stories/status, being a Matrix homeserver, running bots.

## 2. Architecture

```
            ┌──────────────── fili-im daemon (Rust, one per session) ────────────────┐
 window ◄──►│ control socket ── core: store (SQLite + FTS5), people, rules, outbox,  │
 (Qt Quick) │                   notifier, scheduler, read-receipt policy             │
 CLI/agent ◄┤                                                                        │
 fili-bar  ◄┤ alert files (~/.local/state/alerts/messages, messages.count)           │
            └──────┬─────────────────────┬──────────────────────┬────────────────────┘
                   │ stdio, JSON lines   │                      │
         fili-im-whatsapp (Go)   fili-im-telegram (later)   fili-im-fake (tests, UI dev)
            whatsmeow                 …                         scripted
```

### 2.1 One daemon, connectors as separate processes

The core never speaks a platform protocol. Each platform is a **connector**: a small program launched by the daemon,
one process per account, speaking the connector protocol (2.3) on stdin/stdout.

Why separate processes:
- Language freedom: the best WhatsApp library is Go (whatsmeow); Telegram and Matrix have good Rust libraries; Signal
  may need something else. The core stays Rust.
- Isolation: a connector that crashes or is banned does not take the others down; the daemon restarts it with
  back-off and shows the state in the footer and in the bar.
- Updates: an unofficial protocol changes without notice. A connector can be updated alone.
- Cost: only installed connectors with an account run; nothing is loaded for platforms the user does not use.

### 2.2 Capabilities, not platform checks

Same rule as Facteur's providers: platform differences live in the connector's **capability descriptor** and nowhere
else. The window and the bar never test `platform == "whatsapp"`.

The `hello` message of a connector declares, for example:

| Capability | Example values |
|---|---|
| `pairing` | `qr`, `phone_code`, `token`, `oauth`, `password` |
| `message.kinds` | `text`, `image`, `video`, `audio`, `voice`, `file`, `sticker`, `location`, `contact`, `poll` |
| `message.edit` / `message.delete` | `false`, `true`, `{window_s: 900}` |
| `reactions` | `none`, `one_per_user`, `many` |
| `replies` / `threads` | quote reply, real threads (Discord, Slack) |
| `receipts.send` / `receipts.suppress` | can mark read; can stay invisible while reading |
| `typing` / `presence` | send, receive |
| `history.backfill` | none, on pairing, on demand |
| `schedule.native` | the server holds scheduled messages (Telegram) or the daemon does it |
| `media.max_bytes` | per kind |
| `chats.kinds` | `direct`, `group`, `channel`, `server/channel` (Discord) |
| `formatting` | `plain`, `whatsapp_markup`, `markdown`, `html_subset` |
| `identity` | phone numbers visible or not (used to match people, 8.5) |

Capabilities are a ceiling: an account's refusal can remove one, never add one. An action without its capability is
hidden in the UI, missing from the bar menu, and refused by the daemon (hiding a button does not disable its shortcut).

### 2.3 Connector protocol (v1)

JSON lines on stdio, versioned (`protocol: 1`), documented in `docs/connector-protocol.md`, checked by a conformance
suite that runs against every connector and against `fili-im-fake`.

- Connector → daemon events: `hello`, `state` (`connecting`, `connected`, `needs_pairing`, `logged_out`, `banned`,
  `phone_offline`), `pairing` (QR payload or code), `chat.upsert`, `contact.upsert`, `message.new`, `message.edit`,
  `message.delete`, `reaction`, `receipt`, `typing`, `history.batch`, `media.ready`, `error`.
- Daemon → connector commands, each with an id and an `ok`/`error` answer: `pair`, `logout`, `send`, `edit`, `delete`,
  `react`, `mark_read`, `typing`, `fetch_media`, `backfill`, `chat.mute`, `chat.archive`.
- Ids: the daemon names each account `<platform>-<n>` (stable, known before pairing); chats and messages keep their
  native ids, scoped by account and chat. The daemon stores native ids opaque; it never parses them.
- Text arrives in a neutral form: plain text plus spans (bold, italic, code, link, mention). The connector converts
  from and to its platform's markup.

### 2.4 Store

One SQLite database in `~/.local/share/fili-im/` (0600): accounts, chats, contacts, **people** (one person = several
contacts on several platforms), messages, reactions, receipts, drafts, outbox, snoozes, scheduled messages,
notification rules, plus an FTS5 index over message text and file names. Media in
`~/.local/share/fili-im/media/<account>/`, fetched lazily through the connector. Each connector keeps its own session
(WhatsApp keys) in `~/.local/share/fili-im/accounts/<account>/`, 0700, never in logs or argv.

### 2.5 Window and daemon

`fili-im` without arguments opens the window (cxx-qt, same build as Facteur and Casier); `fili-im daemon` runs the
daemon; other subcommands are the CLI. The window talks to the daemon over its control socket
(`$XDG_RUNTIME_DIR/fili-im/daemon.sock`), JSON lines, subscribe + commands. The window has no state the daemon does not
have: closing it loses nothing, two windows show the same thing.

Autostart like Facteur: `spawn-sh-at-startup "command -v fili-im >/dev/null && exec fili-im daemon"` in niri's
config; single instance by `flock`.

## 3. Window

Same anatomy as every `fili-*` application (`agents/skills/filigrane-ui/references/app-window.md`, Casier as reference,
Facteur's footer and tabs), checked by `tests/fili-shell/check-app-window.py`.

- **Header 44**: back/forward, title of the current view and its count, search (Ctrl+F, across all platforms), new
  conversation, Window and Close buttons on the right.
- **Sidebar 200**, foldable section headers:
  - *Inbox*: All, Unread, Mentions, Snoozed, Scheduled.
  - *Pinned*: pinned conversations and people.
  - *Accounts*: one row per account with its platform mark and state (connected, phone offline, needs pairing),
    never by colour alone.
  - *Filters* (later): user lists such as "Family", "Work".
- **Tabs** (band 40, tab 28, Facteur's look): open conversations as document tabs (Ctrl+T, Ctrl+W, Ctrl+Tab), so
  two or three conversations can be followed side by side; the conversation list itself is the first, fixed view.
- **Content**: conversation list column, then the conversation (bubbles, day separators, replies, reactions, media
  thumbnails, typing indicator) and the composer (Shift+Enter sends, Enter new line, paste or drop files, reply
  quote, edit last message with Up when `message.edit` allows it).
- **Footer 28** on `backgroundDeep`: per-account state, operations (sending 3 files…), errors and notices ("WhatsApp:
  the phone has been offline for 9 days, open WhatsApp on it"). Shown, hidden and brought back for an operation as
  the HIG says.
- `KineticScroll` + `FiliScrollBar` on every scrolling view.
- Full menu in the fili-bar (fallback `dotfiles/config/filigrane/appmenu/fili-im.json` + `fili-im ctl --pid PID
  action NAME`), "About Conversations…" (aka fili-im), "Settings…" opening `~/.config/filigrane/fili-im.toml`.
- Header and footer follow Settings > Appearance live; nothing only reachable from the header.

## 4. fili-bar module and notifications

- Module `messages` (`dotfiles/config/quickshell/bar/modules/messages/`): an `AlertModule` (kind `messages`, like
  `mail`) at first: count of unread conversations, tooltip with the last ones, click opens Conversations.
- Then its own panel `MessagesPanel.qml`: last conversations, **peek** (read without receipts, 8.1) and **quick
  reply** (8.2) without opening the window, and per-account state.
- Notifications through `notify-send` (app name Conversations, icon fili-im), grouped per conversation, with actions
  (Open, Reply, Mark read, Mute 1 h). fili-shell's notification centre has no inline reply today: "Reply" opens the
  bar panel on that conversation; inline reply in fili-shell notifications is a separate fili-shell task (P3).
- Do Not Disturb respected; rules decide what breaks through (8.4).

## 5. WhatsApp connector

- `fili-im-whatsapp`, Go, on **whatsmeow** (go.mau.fi/whatsmeow, MPL-2.0), the library behind mautrix-whatsapp and
  Beeper. Linked device: pairing by QR code shown in the window (or pairing code by phone number).
- Facts to design for:
  - The phone stays the primary device; a linked device left offline about 14 days is logged out: the daemon warns
    in the footer and the bar before that.
  - First pairing pushes a history sync (can be large): store it in batches, show progress in the footer, cap it by
    setting.
  - Two id spaces (phone-number JIDs and LIDs) for the same person: the connector maps them and always gives the
    daemon one stable contact id.
  - Read receipts are per message; "read" from the daemon goes only through the policy (8.1).
  - Unofficial protocol: against WhatsApp's terms; ban risk low for normal personal use, not zero. Said in the
    pairing dialog, once.
- Version of whatsmeow pinned; a weekly CI job builds against the latest and opens an issue when it breaks (like
  fili-niri).
- Build: needs `go` at build time only; the xbps package ships the binary.
- Later option: replace it with `whatsapp-rust` when that library is mature, without touching the core.

## 6. Later platforms (vision, so the core is right from the start)

| Platform | Way | Notes |
|---|---|---|
| Telegram | official API, `grammers` (Rust MTProto) or TDLib | Fully allowed. grammers is lighter; TDLib more complete. Choose at P5 by measuring. |
| Signal | `presage` (Rust, on libsignal) as a linked device | signal-cli is Java: avoid resident JVM. |
| Matrix | `matrix-rust-sdk` | Also a gateway: users with mautrix bridges get every other network through it. |
| Discord | user account through the gateway | Automating a user account is against Discord's terms and detected more strictly than WhatsApp: opt-in with a clear warning, or bot-only mode. Decide at that phase. |
| Messenger / Instagram | mautrix-meta's library (Go) | Same model as WhatsApp. |
| Slack | official API with a user token | Workspace admins may forbid it. |
| SMS/RCS | Google Messages pairing (mautrix-gmessages library) | Later. |

Each new platform = one connector + its marks + its capability descriptor; no change in the window if the capability
model holds. That is the test of the architecture.

## 7. Privacy and security

- Messages, names, file names and links are untrusted: plain-text rendering, formatting only from the neutral spans,
  links opened outside, no remote fetch by Qt, link previews off by default (they leak to the link's server).
- Sessions and keys 0600/0700, never in argv, logs or the agent's context.
- Nothing leaves the machine except to the platform itself, unless the user asks: the AI features (8.7) are opt-in
  per conversation and say which agent receives what.
- Security section in every review, as in Facteur (`Security: PASS / BLOCK / NOT VERIFIED`).

## 8. Features that set Conversations apart (vision)

1. **Peek without receipts.** Policy in the daemon: `read_receipts = "on_open" | "manual" | "never"` per account and
   per conversation. The bar panel and notifications never send them; opening the conversation in the window does
   (when `on_open`). Same for typing indicators. Needs `receipts.suppress`.
2. **Answer from the bar or the notification.** Quick reply in the panel, with the last few messages as context.
3. **Snooze and scheduled send.** Snooze a conversation until tonight, tomorrow, Monday, or "when they answer";
   scheduled messages held by the daemon (or by the server when `schedule.native`). Listed in the sidebar.
4. **Notification rules.** Direct messages and mentions now; busy groups as a digest every N minutes; VIP people break
   through Do Not Disturb; quiet hours; per-conversation overrides. Defaults sensible, file-editable.
5. **One person, all platforms.** Contacts merged into people (automatic on matching phone numbers, manual otherwise);
   a person's view shows every platform interleaved; reply goes by the last platform used, changeable.
6. **Local history, owned and searchable.** FTS search across everything; `fili-im search`, `fili-im export` (JSON,
   Markdown); the agent finds "the address Paul sent in March" through the CLI.
7. **AI on demand.** Catch up on a busy group (summary), draft a reply, Mod+Y context gives the open conversation.
   Opt-in per conversation, never automatic; the user approves anything sent.
8. **Files go to Casier.** Received files listed per conversation and per person, "Show in Casier", drag and drop from
   Casier to send, saved with `fili-tx` so undo works.
9. **Voice messages transcribed locally** (whisper.cpp, optional package with a resource warning; the model weighs a
   few hundred MB on disk, loaded only while transcribing).
10. **Keyboard first and light.** Quick switcher (Ctrl+K) over people and conversations, every action in the bar menu
    with its shortcut; budget measured after install.

## 9. Phases

Model per step: **Opus** for new architecture, reverse-engineered protocols, cross-cutting semantics, security and
privacy decisions, and hard debugging; **Sonnet** for work on a fixed contract: UI that follows the established
anatomy, features over the protocol once it exists, porting patterns from Facteur and Casier, tests, packaging. Every
Sonnet phase starts by reading this plan, `docs/connector-protocol.md` and the window anatomy; Opus reviews the diff
at the end of a phase when it touches the protocol or the store.

| Phase | Content | Model | Why |
|---|---|---|---|
| **P0 Foundations** | Repository, Cargo workspace (core lib, daemon, CLI, gui feature), `docs/connector-protocol.md`, capability schema, SQLite schema + migrations, daemon skeleton (single instance, socket, connector supervisor with back-off, alert files, notify-send), `fili-im-fake` connector, conformance suite | **Opus** | Every later phase builds on these contracts; mistakes here are expensive. |
| **P1 WhatsApp connector** | `fili-im-whatsapp`: pairing QR/code, connection states, chats/contacts, text in/out, receipts, history sync in batches, JID/LID mapping, media download/upload, reactions, edit/delete; CLI `fili-im pair`, `fili-im send` to test without a window | **Opus** (protocol mapping, ids, history), **Sonnet** for media kinds and conformance tests once text works | Unofficial protocol and identity mapping are the risky part. |
| **P2 Window v0** | Anatomy (header, sidebar, tabs, footer), conversation list, conversation view, composer, pairing dialog, bar menu fallback + `ctl`, About, Settings, theme tokens, `check-app-window.py` passing | **Sonnet** | Copies Facteur/Casier patterns over a fixed daemon API. Opus only if the window↔daemon API needs a change. |
| **P3 Bar and notifications** | `messages` module (AlertModule, then panel with peek + quick reply), notifications with actions, receipt policy (8.1), per-account states in the bar | **Sonnet**; **Opus** for the receipt policy in the daemon and for inline reply in fili-shell's notifications | QML follows existing modules; the policy is privacy-critical. |
| **Live test + tag v0.1.0** | Thomas uses it a few days on his WhatsApp; fixes; `filigrane-os/fili-im` on GitHub, tag, `apps/fili-im/upstream`, package (with Go build), niri autostart, budget measure | Sonnet | Release mechanics already exist for the other apps. |
| **P4 Daily features** | Snooze, scheduled send, notification rules + digests + VIP, FTS search and Ctrl+K switcher, media gallery and Casier integration | **Sonnet** | Features over the store; no protocol change except `schedule.native`. |
| **P5 Telegram** | Measure grammers vs TDLib (Opus, short), then connector | **Opus** for the choice and the first slice, **Sonnet** to complete | Validates the capability model on a second platform; protocol changes, if any, are decided here. |
| **P6 People** | Merging contacts into people (auto by phone, manual), person view, reply-by-last-platform | **Opus** | Matching heuristics and data model migration. |
| **P7 Agent and voice** | `fili-im search/export` for the agent, Mod+Y context, opt-in summaries and reply drafts, local voice transcription | **Opus** for the privacy design, **Sonnet** for the implementation | Messages leaving the machine must be designed carefully. |
| **P8+ More platforms** | Signal, Matrix, Discord (decision on terms), Messenger | **Sonnet** per connector on the proven model, **Opus** for each terms/identity decision | Mostly mapping work once two connectors exist. |

## 10. Open decisions

- Display name: "Conversations" (chosen 2026-11-15).
- Telegram library: grammers or TDLib (P5, by measurement).
- Discord: user-account connector with a warning, bot-only, or not at all (P8).
- Inline reply in fili-shell notifications: implement in fili-shell (benefits Facteur too) or keep "Reply opens the
  panel".
