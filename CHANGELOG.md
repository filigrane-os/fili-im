# Changelog

## Unreleased

- Phase P1, WhatsApp connector (`connectors/whatsapp`, Go, whatsmeow): pairing by QR code or phone code, text in and
  out with WhatsApp markup as spans, replies, reactions, edits, deletes for everyone, delivery and read receipts,
  typing, mute and archive, history sync in batches, contacts and groups, LID ids mapped to phone numbers, media
  received (downloaded on demand). Sending files is not done yet.
- `fili-im pair` draws the QR code in the terminal.

- Phase P0, foundations: connector protocol v1 (`docs/connector-protocol.md`), capability model, SQLite store with
  FTS5 search, daemon (connector supervision with back-off, outbox, notifications, fili-bar alert files, control
  socket), CLI, `fili-im-fake` connector, conformance suite.
