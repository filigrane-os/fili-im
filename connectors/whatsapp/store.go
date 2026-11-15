// The connector's own small database (connector.db, next to whatsmeow's session): idempotent sends, and what replies,
// reactions, receipts and downloads need to know about recent messages.
package main

import (
	"context"
	"database/sql"
	"errors"
	"time"

	"google.golang.org/protobuf/proto"

	"go.mau.fi/whatsmeow/proto/waE2E"
)

const schema = `
CREATE TABLE IF NOT EXISTS sent (client_id TEXT PRIMARY KEY, msg_id TEXT NOT NULL, ts INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS msgs (
    chat TEXT NOT NULL, id TEXT NOT NULL, raw_chat TEXT NOT NULL, raw_sender TEXT NOT NULL,
    from_me INTEGER NOT NULL, ts INTEGER NOT NULL, body TEXT NOT NULL DEFAULT '', media BLOB,
    PRIMARY KEY (chat, id)
);
CREATE INDEX IF NOT EXISTS msgs_chat_ts ON msgs(chat, ts);
`

// Messages older than this are forgotten here (the daemon keeps them): replies and reactions to them still work in
// WhatsApp, just without the quoted text.
const keepFor = 120 * 24 * time.Hour

type Local struct {
	db *sql.DB
}

func OpenLocal(path string) (*Local, error) {
	db, err := sql.Open("sqlite3", "file:"+path+"?_busy_timeout=5000&_journal_mode=WAL")
	if err != nil {
		return nil, err
	}
	if _, err := db.Exec(schema); err != nil {
		db.Close()
		return nil, err
	}
	return &Local{db: db}, nil
}

func (l *Local) Close() { l.db.Close() }

func (l *Local) SentID(ctx context.Context, clientID string) (string, bool) {
	var id string
	err := l.db.QueryRowContext(ctx, "SELECT msg_id FROM sent WHERE client_id = ?", clientID).Scan(&id)
	return id, err == nil
}

func (l *Local) RememberSent(ctx context.Context, clientID, msgID string) error {
	_, err := l.db.ExecContext(ctx, "INSERT OR IGNORE INTO sent (client_id, msg_id, ts) VALUES (?, ?, ?)", clientID, msgID, time.Now().UnixMilli())
	return err
}

type known struct {
	RawChat   string
	RawSender string
	FromMe    bool
	Body      string
	Timestamp int64
}

func (l *Local) Remember(ctx context.Context, chat, id, rawChat, rawSender string, fromMe bool, ts int64, body string, media *waE2E.Message) {
	var blob []byte
	if media != nil {
		blob, _ = proto.Marshal(media)
	}
	_, _ = l.db.ExecContext(ctx, `INSERT INTO msgs (chat, id, raw_chat, raw_sender, from_me, ts, body, media) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
		ON CONFLICT (chat, id) DO UPDATE SET body = excluded.body, media = COALESCE(excluded.media, media)`,
		chat, id, rawChat, rawSender, fromMe, ts, body, blob)
}

func (l *Local) Edited(ctx context.Context, chat, id, body string) {
	_, _ = l.db.ExecContext(ctx, "UPDATE msgs SET body = ? WHERE chat = ? AND id = ?", body, chat, id)
}

func (l *Local) Known(ctx context.Context, chat, id string) (known, bool) {
	var k known
	err := l.db.QueryRowContext(ctx, "SELECT raw_chat, raw_sender, from_me, body, ts FROM msgs WHERE chat = ? AND id = ?", chat, id).
		Scan(&k.RawChat, &k.RawSender, &k.FromMe, &k.Body, &k.Timestamp)
	return k, err == nil
}

// Latest message of a chat (archiving needs it).
func (l *Local) Latest(ctx context.Context, chat string) (string, known, bool) {
	var id string
	var k known
	err := l.db.QueryRowContext(ctx, "SELECT id, raw_chat, raw_sender, from_me, body, ts FROM msgs WHERE chat = ? ORDER BY ts DESC LIMIT 1", chat).
		Scan(&id, &k.RawChat, &k.RawSender, &k.FromMe, &k.Body, &k.Timestamp)
	return id, k, err == nil
}

var errNoMedia = errors.New("no media for this message")

func (l *Local) Media(ctx context.Context, chat, id string) (*waE2E.Message, error) {
	var blob []byte
	err := l.db.QueryRowContext(ctx, "SELECT media FROM msgs WHERE chat = ? AND id = ?", chat, id).Scan(&blob)
	if err != nil || len(blob) == 0 {
		return nil, errNoMedia
	}
	var m waE2E.Message
	if err := proto.Unmarshal(blob, &m); err != nil {
		return nil, err
	}
	return &m, nil
}

func (l *Local) Prune(ctx context.Context) {
	cutoff := time.Now().Add(-keepFor).UnixMilli()
	_, _ = l.db.ExecContext(ctx, "DELETE FROM msgs WHERE ts < ?", cutoff)
	_, _ = l.db.ExecContext(ctx, "DELETE FROM sent WHERE ts < ?", cutoff)
}
