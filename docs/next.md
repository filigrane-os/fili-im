# Next steps (handoff)

Updated 2026-11-15, after P0 and P1.

## State

- P0 and P1 are committed. `make check` passes (Rust, Go, conformance of `fili-im-whatsapp`).
- The user's WhatsApp is paired as `whatsapp-1` (126 chats, about 5,500 messages of history), on the user's real
  `~/.local/share/fili-im`. The daemon runs detached:
  `setsid -f target/release/fili-im daemon >/dev/null 2>>~/.local/state/fili-im/daemon.log </dev/null`.
  Started from a `!` prompt command it dies with that shell. There is no niri autostart yet (P3).
- To deploy a new connector build: `make connectors-release`, then kill the connector process (comm
  `fili-im-whatsap`). The daemon restarts it within a second and the session is kept. Never kill the daemon without
  restarting it as above.

## Still open from P1 (live test by the user, not done yet)

1. A message received on the phone gives a desktop notification.
2. `fili-im send N "…"` reaches WhatsApp on the phone.
3. Reading a chat on the phone clears its unread count here.

If one fails: read `~/.local/state/fili-im/whatsapp-1.log` and `daemon.log`, then fix in `connectors/whatsapp`
(Opus if the fix touches identity, LIDs or receipts).

Not done in P1: sending files (`message_kinds` stays `["text"]` until then), on-demand backfill.

## P2: the window (Sonnet)

Read first: `docs/plan.md` §3, `AGENTS.md`, and Filigrane's `agents/skills/filigrane-ui/` (SKILL.md, then
`references/app-window.md`, `tokens.md`, `hig.md`).

- The same binary `fili-im` opens the window when run without a subcommand (the `gui` feature, cxx-qt 0.10, as in
  Facteur: `filigrane-os/fili-mail`, `src/gui/` and `build.rs`; build with `QMAKE=/usr/lib/qt6/bin/qmake6`).
  The tests stay `--no-default-features`.
- The window is a client of the daemon's control socket (JSON lines, `subscribe` for events, see
  `src/daemon/control.rs`), never of the store directly. Use the async socket in a Rust thread and send the events
  to QML.
- Anatomy: header 44, sidebar 200 (Inbox: All, Unread; Accounts with their state), tabs (open conversations),
  conversation list, conversation view, composer, footer 28 (account states, errors). Copy `FiliTheme.qml`,
  `KineticScroll.qml`, `FiliScrollBar.qml` and `AboutCard.qml` from Casier/Facteur;
  `tests/fili-shell/check-app-window.py` must pass.
- A pairing dialog shows the QR code (`pairing` events) or asks for the phone number.
- Bar menu: fallback file `dotfiles/config/filigrane/appmenu/fili-im.json` in Filigrane, plus `fili-im ctl --pid PID
  action NAME`; "About Conversations…" (aka fili-im); "Settings…" opens `~/.config/filigrane/fili-im.toml`.
- Opening a conversation calls `mark_read` with `receipt: false` for now (the receipt policy is P3).
- Test it live with the user before tagging anything.

## P2 state (2026-11-15)

The window is built and committed: sidebar (Inbox, Accounts, foldable), conversation list, tabs, conversation view,
composer (send, reply, edit last message), pairing dialog (QR drawn from `qrRows`), footer, About card. The Rust side is
in the lib crate (`src/gui/`, behind the `gui` feature: cxx-qt links its objects into every binary of the package, so the
bridge must live in the lib); `fili-im ctl` commands the window (sockets in `$XDG_RUNTIME_DIR/fili-im/windows/`).
Seen working on the real account (126 conversations). `check-app-window.py` passes; the fallback menu is in Filigrane.

Not done: live test by the user of opening a conversation / sending / receiving, Settings file (`fili-im.toml` does not
exist yet: the "Settings…" entry opens a missing file; create it only when there is a real setting), French catalog
(`i18n/fr.json` is empty), media thumbnails (shown as a labelled chip; the daemon has no `fetch_media` control method
yet), reactions sending, deployment (`./bootstrap.sh --apps`, pinned in Filigrane `apps/`), a shortcuts card.
