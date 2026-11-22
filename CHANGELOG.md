# Changelog

## 0.3.0 - 2026-11-22

- The assistant is now named Lia: the header button and the menu entry say "Ask Lia", and the hidden brief is `shell/lia/<app_id>.md` (the older `rob/` is still read).
- A catalog of the commands of the window (`shell/ctl/<app_id>.json`, `fili-api show`) that follows the shared format; the assistant is driven by `fili-ai` as one client of the Filigrane contracts.

## 0.2.0 - 2026-11-21

- Everything Conversations brings to the desktop now lives in this repository and is installed with it (`make install`): the launcher and icon, and what the Filigrane shell reads, in `shell/`, instead of being kept in the distribution's repository: the `messages` module of the fili-bar (`shell/fili-bar/messages/`, with its French translations), the fallback menu of the bar (`shell/appmenu/fili-im.json`) and the brief of Rob for this application (`shell/rob/fili-im.md`). The package puts it in `/usr/share/filigrane/shell/`, `fili-bar link-system` (run at login) links it into the bar and the menus; a module, menu or brief of your own with the same name wins.
- New `make install-data` (everything but the programs). `scripts/check-shell-menu.py` (run by `make test`, so by the CI) checks that the bar menu agrees with the actions and shortcuts of the window, and that the brief of Rob exists.

## 0.1.5 - 2026-11-19

- The window asks for attention: the header and footer take the accent colour when a message arrives while another window is in front (`[window] attention`, `--no-attention`).
- Header: an Ask Rob button (left of the window button) shows Rob's small floating field for Conversations (`fili-chat here`).
- Quick Look and video links follow the default audio output of the system instead of the one found when the player was created.
- Command line: `fili-im chats --find NAME` filters conversations by a part of the title (case and accents ignored), and `fili-im send` accepts a part of the title instead of the number: one match sends, several list the candidates and send nothing.

## 0.1.4 - 2026-11-18

- Debug panel (View > Debug Logs, Ctrl+Shift+L): the window's log, selectable, above the footer; Copy, Open Log,
  Clear, Ask Rob. The window logs to `~/.local/state/filigrane/logs/fili-im.log`, the daemon to `fili-im-daemon.log`.
- A real bug (panic, QML error, Ask Rob) becomes an incident through `fili-incident` (`[debug] incidents = "off"` in
  `fili-im.toml` turns it off).
- View > Restart Service: the daemon and its connectors start afresh, the window reconnects on its own.

## 0.1.3 - 2026-11-16

- `make release` builds only the `fili-im` program of the package (the packaging CI could not link `fili-im-fake`).

## 0.1.2 - 2026-11-16

- A conversation view that stays put: messages go through a ListModel updated in place (no rebuild of every bubble
  when one comes in or changes), the view stays at the very end after a send or a receipt until you scroll.

- Clicking a quoted message (a reply) goes to the original message and flashes it.
- Pictures in a message are shown in cover mode: a tall picture fills its box (cropped) instead of floating in black
  bars; Quick Look still shows the whole picture.

## 0.1.1 - 2026-11-16

- A conversation opens at the very bottom (margin included) and stays there while its pictures and previews load,
  until you scroll.
- Packaging: `make install` (programs, desktop entry, icon, licenses) and `package.env` for Filigrane's xbps package
  and `bootstrap.sh`.

## 0.1.0 - 2026-11-16

First release: WhatsApp and Google Messages in one window.

- Window (Qt Quick): inbox filters, accounts, conversation list, conversation tabs, composer, footer, pairing dialog
  that opens at once and shows the code, About card, bar menu, link previews (Open Graph, YouTube and other video
  pages through yt-dlp), Quick Look, voice messages, contact cards, status menu.
- Accounts: right click on an account removes it (unlinks the device, deletes its messages after a confirmation);
  "Add Account…" lists the installed connectors.
- Google Messages connector (`connectors/gmessages`, Go, libgm from mautrix-gmessages, AGPL-3.0): SMS, MMS and RCS of
  an Android phone, linked like Messages for web. Pairing signs in to the Google account in a throw-away browser
  profile (no automation switches, which Google refuses) and confirms an emoji on the phone; the old QR code is kept
  as the second method. Text, images, videos and files, replies, reactions, delete, read marks, typing, media
  download.
- Message text has a looser line spacing; the view always follows the last message, sent or received.
- Daemon: a command sent to a connector that has just started waits for its hello.

## Earlier work (before 0.1.0)

- Phase P1, WhatsApp connector (`connectors/whatsapp`, Go, whatsmeow): pairing by QR code or phone code, text in and
  out with WhatsApp markup as spans, replies, reactions, edits, deletes for everyone, delivery and read receipts,
  typing, mute and archive, history sync in batches, contacts and groups, LID ids mapped to phone numbers, media
  received (downloaded on demand). Sending files is not done yet.
- `fili-im pair` draws the QR code in the terminal.

- Phase P0, foundations: connector protocol v1 (`docs/connector-protocol.md`), capability model, SQLite store with
  FTS5 search, daemon (connector supervision with back-off, outbox, notifications, fili-bar alert files, control
  socket), CLI, `fili-im-fake` connector, conformance suite.
