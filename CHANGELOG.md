# Changelog

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
