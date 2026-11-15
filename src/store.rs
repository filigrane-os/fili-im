//! The store: one SQLite database for every account (accounts, chats, contacts, people, messages, reactions), with an
//! FTS5 index over message text. The daemon is its only writer; views come out as JSON for the window and the CLI.
use crate::protocol::{Capabilities, Chat, Contact, Message, ReceiptKind, SelfInfo, Text};
use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde_json::{Value, json};
use std::path::Path;

const SCHEMA_1: &str = r#"
CREATE TABLE accounts(
    key TEXT PRIMARY KEY,
    platform TEXT NOT NULL,
    name TEXT NOT NULL DEFAULT '',
    self_id TEXT,
    self_name TEXT,
    state TEXT NOT NULL DEFAULT 'connecting',
    detail TEXT,
    capabilities TEXT NOT NULL DEFAULT '{}',
    created INTEGER NOT NULL
);
CREATE TABLE people(
    id INTEGER PRIMARY KEY,
    name TEXT
);
CREATE TABLE contacts(
    id INTEGER PRIMARY KEY,
    account TEXT NOT NULL REFERENCES accounts(key) ON DELETE CASCADE,
    native_id TEXT NOT NULL,
    name TEXT,
    phone TEXT,
    person INTEGER REFERENCES people(id) ON DELETE SET NULL,
    UNIQUE(account, native_id)
);
CREATE TABLE chats(
    id INTEGER PRIMARY KEY,
    account TEXT NOT NULL REFERENCES accounts(key) ON DELETE CASCADE,
    native_id TEXT NOT NULL,
    kind TEXT NOT NULL DEFAULT 'direct',
    title TEXT,
    unread INTEGER NOT NULL DEFAULT 0,
    muted_until INTEGER NOT NULL DEFAULT 0,
    archived INTEGER NOT NULL DEFAULT 0,
    pinned INTEGER NOT NULL DEFAULT 0,
    last_ts INTEGER NOT NULL DEFAULT 0,
    last_read TEXT,
    UNIQUE(account, native_id)
);
CREATE TABLE messages(
    id INTEGER PRIMARY KEY,
    chat INTEGER NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
    native_id TEXT,
    client_id TEXT UNIQUE,
    sender TEXT,
    from_me INTEGER NOT NULL DEFAULT 0,
    ts INTEGER NOT NULL,
    kind TEXT NOT NULL DEFAULT 'text',
    body TEXT NOT NULL DEFAULT '',
    spans TEXT,
    reply_to TEXT,
    media TEXT,
    edited_ts INTEGER,
    deleted INTEGER NOT NULL DEFAULT 0,
    state TEXT NOT NULL DEFAULT 'received',
    error TEXT,
    UNIQUE(chat, native_id)
);
CREATE INDEX messages_chat_ts ON messages(chat, ts);
CREATE TABLE reactions(
    message INTEGER NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    sender TEXT NOT NULL,
    emoji TEXT NOT NULL,
    ts INTEGER,
    PRIMARY KEY(message, sender)
);
CREATE VIRTUAL TABLE messages_fts USING fts5(
    body, content='messages', content_rowid='id', tokenize='unicode61 remove_diacritics 2'
);
CREATE TRIGGER messages_fts_insert AFTER INSERT ON messages BEGIN
    INSERT INTO messages_fts(rowid, body) VALUES (new.id, new.body);
END;
CREATE TRIGGER messages_fts_delete AFTER DELETE ON messages BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, body) VALUES ('delete', old.id, old.body);
END;
CREATE TRIGGER messages_fts_update AFTER UPDATE OF body ON messages BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, body) VALUES ('delete', old.id, old.body);
    INSERT INTO messages_fts(rowid, body) VALUES (new.id, new.body);
END;
"#;

/// Migrations in order; `PRAGMA user_version` is the number applied.
const MIGRATIONS: &[&str] = &[SCHEMA_1];

pub fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// SQL condition: the chat `c` is not muted now (`?now` is bound by the caller as the named parameter `:now`).
const NOT_MUTED: &str = "(c.muted_until = 0 OR (c.muted_until > 0 AND c.muted_until < :now))";
/// SQL expression: the chat `c`'s title, falling back to the contact of a direct chat, then to its id.
const CHAT_TITLE: &str = "COALESCE(c.title, (SELECT COALESCE(ct.name, ct.phone) FROM contacts ct \
     WHERE ct.account = c.account AND ct.native_id = c.native_id), c.native_id)";
/// SQL expression: the sender of message `m` in chat `c`.
const SENDER_NAME: &str = "(SELECT COALESCE(ct.name, ct.phone) FROM contacts ct \
     WHERE ct.account = c.account AND ct.native_id = m.sender)";

/// A message to send, as the outbox gives it back.
#[derive(Debug, Clone, PartialEq)]
pub struct Outgoing {
    pub id: i64,
    pub client_id: String,
    pub chat: String,
    pub text: Text,
    pub reply_to: Option<String>,
}

/// What `mark_read` found to acknowledge on the platform.
#[derive(Debug, Clone, PartialEq)]
pub struct ReadMark {
    pub account: String,
    pub chat: String,
    pub kind: String,
    /// (message id, sender) of the incoming messages read now, oldest first.
    pub messages: Vec<(String, Option<String>)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stored {
    pub id: i64,
    pub chat: i64,
    pub new: bool,
}

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Store> {
        use std::os::unix::fs::PermissionsExt;
        if let Some(dir) = path.parent() {
            crate::paths::private_dir(dir).with_context(|| format!("cannot create {}", dir.display()))?;
        }
        let conn = Connection::open(path).with_context(|| format!("cannot open {}", path.display()))?;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        conn.pragma_update(None, "journal_mode", "WAL")?;
        Store::init(conn)
    }

    pub fn open_in_memory() -> Result<Store> {
        Store::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Store> {
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        let store = Store { conn };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<()> {
        let applied = self.conn.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))? as usize;
        if applied > MIGRATIONS.len() {
            anyhow::bail!("the database was made by a newer fili-im (schema {applied})");
        }
        for (n, sql) in MIGRATIONS.iter().enumerate().skip(applied) {
            let tx = self.conn.unchecked_transaction()?;
            tx.execute_batch(sql).with_context(|| format!("migration {}", n + 1))?;
            tx.pragma_update(None, "user_version", (n + 1) as i64)?;
            tx.commit()?;
        }
        Ok(())
    }

    // -----------------------------------------------------------------------------------------------------------
    // Accounts

    /// A new account `<platform>-<n>`, n one past the highest ever used for that platform still in the table.
    pub fn add_account(&self, platform: &str) -> Result<String> {
        let prefix = format!("{platform}-");
        let mut stmt = self.conn.prepare("SELECT key FROM accounts WHERE platform = ?1")?;
        let highest = stmt
            .query_map([platform], |r| r.get::<_, String>(0))?
            .flatten()
            .filter_map(|key| key.strip_prefix(&prefix).and_then(|n| n.parse::<u32>().ok()))
            .max()
            .unwrap_or(0);
        let key = format!("{platform}-{}", highest + 1);
        self.conn
            .execute("INSERT INTO accounts(key, platform, created) VALUES (?1, ?2, ?3)", params![key, platform, now_ms()])?;
        Ok(key)
    }

    /// (key, platform) of every account.
    pub fn account_keys(&self) -> Result<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare("SELECT key, platform FROM accounts ORDER BY created, key")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    fn account_view(r: &Row) -> rusqlite::Result<Value> {
        let caps: String = r.get("capabilities")?;
        Ok(json!({
            "key": r.get::<_, String>("key")?,
            "platform": r.get::<_, String>("platform")?,
            "name": r.get::<_, String>("name")?,
            "self_id": r.get::<_, Option<String>>("self_id")?,
            "self_name": r.get::<_, Option<String>>("self_name")?,
            "state": r.get::<_, String>("state")?,
            "detail": r.get::<_, Option<String>>("detail")?,
            "capabilities": serde_json::from_str::<Value>(&caps).unwrap_or(Value::Null),
        }))
    }

    pub fn accounts(&self) -> Result<Vec<Value>> {
        let mut stmt = self.conn.prepare("SELECT * FROM accounts ORDER BY created, key")?;
        let rows = stmt.query_map([], Store::account_view)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn account(&self, key: &str) -> Result<Option<Value>> {
        Ok(self.conn.query_row("SELECT * FROM accounts WHERE key = ?1", [key], Store::account_view).optional()?)
    }

    pub fn set_hello(&self, key: &str, name: &str, caps: &Capabilities) -> Result<()> {
        self.conn.execute(
            "UPDATE accounts SET name = ?2, capabilities = ?3 WHERE key = ?1",
            params![key, name, serde_json::to_string(caps)?],
        )?;
        Ok(())
    }

    pub fn capabilities(&self, key: &str) -> Result<Capabilities> {
        let caps: Option<String> =
            self.conn.query_row("SELECT capabilities FROM accounts WHERE key = ?1", [key], |r| r.get(0)).optional()?;
        Ok(caps.and_then(|c| serde_json::from_str(&c).ok()).unwrap_or_default())
    }

    pub fn set_state(&self, key: &str, state: &str, detail: Option<&str>, me: Option<&SelfInfo>) -> Result<()> {
        self.conn.execute("UPDATE accounts SET state = ?2, detail = ?3 WHERE key = ?1", params![key, state, detail])?;
        if let Some(me) = me {
            self.conn.execute(
                "UPDATE accounts SET self_id = ?2, self_name = COALESCE(?3, self_name) WHERE key = ?1",
                params![key, me.id, me.name],
            )?;
        }
        Ok(())
    }

    pub fn remove_account(&self, key: &str) -> Result<bool> {
        Ok(self.conn.execute("DELETE FROM accounts WHERE key = ?1", [key])? > 0)
    }

    // -----------------------------------------------------------------------------------------------------------
    // Chats and contacts

    /// The chat's row, created if the connector mentions a chat it has not announced.
    fn chat_row(&self, account: &str, native: &str) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO chats(account, native_id) VALUES (?1, ?2) ON CONFLICT(account, native_id) DO NOTHING",
            params![account, native],
        )?;
        Ok(self
            .conn
            .query_row("SELECT id FROM chats WHERE account = ?1 AND native_id = ?2", params![account, native], |r| r.get(0))?)
    }

    pub fn find_chat(&self, account: &str, native: &str) -> Result<Option<i64>> {
        Ok(self
            .conn
            .query_row("SELECT id FROM chats WHERE account = ?1 AND native_id = ?2", params![account, native], |r| r.get(0))
            .optional()?)
    }

    /// Absent fields keep their stored value.
    pub fn upsert_chat(&self, account: &str, chat: &Chat) -> Result<i64> {
        let id = self.chat_row(account, &chat.id)?;
        self.conn.execute(
            "UPDATE chats SET kind = COALESCE(?2, kind), title = COALESCE(?3, title), unread = COALESCE(?4, unread),
                 muted_until = COALESCE(?5, muted_until), archived = COALESCE(?6, archived),
                 pinned = COALESCE(?7, pinned), last_read = COALESCE(?8, last_read)
             WHERE id = ?1",
            params![id, chat.kind, chat.title, chat.unread, chat.muted_until, chat.archived, chat.pinned, chat.last_read],
        )?;
        Ok(id)
    }

    pub fn upsert_contact(&self, account: &str, contact: &Contact) -> Result<()> {
        self.conn.execute(
            "INSERT INTO contacts(account, native_id, name, phone) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(account, native_id) DO UPDATE SET
                 name = COALESCE(excluded.name, name), phone = COALESCE(excluded.phone, phone)",
            params![account, contact.id, contact.name, contact.phone],
        )?;
        Ok(())
    }

    /// (account, native chat id, muted now) of a chat row.
    pub fn chat_ref(&self, id: i64) -> Result<Option<(String, String, bool)>> {
        let sql = format!("SELECT account, native_id, NOT {NOT_MUTED} FROM chats c WHERE id = :id");
        Ok(self
            .conn
            .query_row(&sql, rusqlite::named_params! {":id": id, ":now": now_ms()}, |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .optional()?)
    }

    fn chat_query(filter: &str) -> String {
        format!(
            "SELECT c.*, {CHAT_TITLE} AS shown_title, NOT {NOT_MUTED} AS muted_now,
                 (SELECT m.id FROM messages m WHERE m.chat = c.id ORDER BY m.ts DESC, m.id DESC LIMIT 1) AS last_message
             FROM chats c WHERE {filter}"
        )
    }

    fn chat_view(&self, r: &Row) -> rusqlite::Result<Value> {
        let last: Option<i64> = r.get("last_message")?;
        let last = last.and_then(|id| self.message_view(id).ok().flatten());
        Ok(json!({
            "id": r.get::<_, i64>("id")?,
            "account": r.get::<_, String>("account")?,
            "native_id": r.get::<_, String>("native_id")?,
            "kind": r.get::<_, String>("kind")?,
            "title": r.get::<_, String>("shown_title")?,
            "unread": r.get::<_, i64>("unread")?,
            "muted": r.get::<_, bool>("muted_now")?,
            "muted_until": r.get::<_, i64>("muted_until")?,
            "archived": r.get::<_, bool>("archived")?,
            "pinned": r.get::<_, bool>("pinned")?,
            "last_ts": r.get::<_, i64>("last_ts")?,
            "last": last,
        }))
    }

    pub fn chat(&self, id: i64) -> Result<Option<Value>> {
        let sql = Store::chat_query("c.id = :id");
        let mut stmt = self.conn.prepare(&sql)?;
        Ok(stmt.query_row(rusqlite::named_params! {":id": id, ":now": now_ms()}, |r| self.chat_view(r)).optional()?)
    }

    /// Pinned first, then the most recent; `account` and `unread_only` narrow the list.
    pub fn chats(&self, account: Option<&str>, unread_only: bool, limit: i64) -> Result<Vec<Value>> {
        let sql = format!(
            "{} ORDER BY c.pinned DESC, c.last_ts DESC, c.id DESC LIMIT :limit",
            Store::chat_query("(:account IS NULL OR c.account = :account) AND (NOT :unread OR c.unread > 0)")
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(
            rusqlite::named_params! {":account": account, ":unread": unread_only, ":limit": limit, ":now": now_ms()},
            |r| self.chat_view(r),
        )?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    // -----------------------------------------------------------------------------------------------------------
    // Messages

    fn put(&self, account: &str, m: &Message, live: bool) -> Result<Stored> {
        let chat = self.chat_row(account, &m.chat)?;
        let body = m.text.as_ref().map(|t| t.plain.as_str()).unwrap_or("");
        let spans = m.text.as_ref().filter(|t| !t.spans.is_empty()).map(|t| serde_json::to_string(&t.spans)).transpose()?;
        let media = m.media.as_ref().map(serde_json::to_string).transpose()?;
        // Our own message coming back (echo): it is the pending one.
        if let Some(client) = &m.client_id
            && let Some(id) = self
                .conn
                .query_row("SELECT id FROM messages WHERE client_id = ?1 AND chat = ?2", params![client, chat], |r| {
                    r.get::<_, i64>(0)
                })
                .optional()?
        {
            let taken: Option<i64> = self
                .conn
                .query_row(
                    "SELECT id FROM messages WHERE chat = ?1 AND native_id = ?2 AND id != ?3",
                    params![chat, m.id, id],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(other) = taken {
                self.conn.execute("DELETE FROM messages WHERE id = ?1", [id])?;
                return Ok(Stored { id: other, chat, new: false });
            }
            self.conn.execute(
                "UPDATE messages SET native_id = ?2, ts = ?3,
                     state = CASE WHEN state IN ('pending', 'failed') THEN 'sent' ELSE state END, error = NULL
                 WHERE id = ?1",
                params![id, m.id, m.timestamp],
            )?;
            return Ok(Stored { id, chat, new: false });
        }
        let existing: Option<i64> = self
            .conn
            .query_row("SELECT id FROM messages WHERE chat = ?1 AND native_id = ?2", params![chat, m.id], |r| r.get(0))
            .optional()?;
        if let Some(id) = existing {
            self.conn.execute(
                "UPDATE messages SET kind = ?2, body = ?3, spans = ?4, media = COALESCE(?5, media),
                     reply_to = COALESCE(?6, reply_to)
                 WHERE id = ?1 AND deleted = 0",
                params![id, m.kind, body, spans, media, m.reply_to],
            )?;
            return Ok(Stored { id, chat, new: false });
        }
        self.conn.execute(
            "INSERT INTO messages(chat, native_id, sender, from_me, ts, kind, body, spans, reply_to, media, state)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                chat,
                m.id,
                if m.from_me { None } else { m.sender.as_deref() },
                m.from_me,
                m.timestamp,
                m.kind,
                body,
                spans,
                m.reply_to,
                media,
                if m.from_me { "sent" } else { "received" }
            ],
        )?;
        let id = self.conn.last_insert_rowid();
        self.conn.execute("UPDATE chats SET last_ts = MAX(last_ts, ?2) WHERE id = ?1", params![chat, m.timestamp])?;
        if live && !m.from_me {
            self.conn.execute("UPDATE chats SET unread = unread + 1 WHERE id = ?1", [chat])?;
        }
        Ok(Stored { id, chat, new: true })
    }

    pub fn put_message(&self, account: &str, m: &Message, live: bool) -> Result<Stored> {
        let tx = self.conn.unchecked_transaction()?;
        let stored = self.put(account, m, live)?;
        tx.commit()?;
        Ok(stored)
    }

    /// History, in one transaction; never counted as unread (the platform's count comes with `chat.upsert`).
    pub fn put_history(&self, account: &str, messages: &[Message]) -> Result<Vec<Stored>> {
        let tx = self.conn.unchecked_transaction()?;
        let stored = messages.iter().map(|m| self.put(account, m, false)).collect::<Result<Vec<_>>>()?;
        tx.commit()?;
        Ok(stored)
    }

    fn message_id(&self, account: &str, chat: &str, native: &str) -> Result<Option<i64>> {
        let Some(chat) = self.find_chat(account, chat)? else { return Ok(None) };
        Ok(self
            .conn
            .query_row("SELECT id FROM messages WHERE chat = ?1 AND native_id = ?2", params![chat, native], |r| r.get(0))
            .optional()?)
    }

    pub fn edit_message(&self, account: &str, chat: &str, native: &str, text: &Text, ts: Option<i64>) -> Result<Option<i64>> {
        let Some(id) = self.message_id(account, chat, native)? else { return Ok(None) };
        let spans = (!text.spans.is_empty()).then(|| serde_json::to_string(&text.spans)).transpose()?;
        self.conn.execute(
            "UPDATE messages SET body = ?2, spans = ?3, edited_ts = ?4 WHERE id = ?1 AND deleted = 0",
            params![id, text.plain, spans, ts.unwrap_or_else(now_ms)],
        )?;
        Ok(Some(id))
    }

    /// Deleted messages stay as a placeholder ("This message was deleted"), without their content.
    pub fn delete_message(&self, account: &str, chat: &str, native: &str) -> Result<Option<i64>> {
        let Some(id) = self.message_id(account, chat, native)? else { return Ok(None) };
        self.conn.execute(
            "UPDATE messages SET deleted = 1, body = '', spans = NULL, media = NULL, reply_to = NULL WHERE id = ?1",
            [id],
        )?;
        self.conn.execute("DELETE FROM reactions WHERE message = ?1", [id])?;
        Ok(Some(id))
    }

    /// `sender` None = me. An empty emoji removes the reaction.
    pub fn react(
        &self,
        account: &str,
        chat: &str,
        native: &str,
        sender: Option<&str>,
        emoji: &str,
        ts: Option<i64>,
    ) -> Result<Option<i64>> {
        let Some(id) = self.message_id(account, chat, native)? else { return Ok(None) };
        let sender = sender.unwrap_or("");
        if emoji.is_empty() {
            self.conn.execute("DELETE FROM reactions WHERE message = ?1 AND sender = ?2", params![id, sender])?;
        } else {
            self.conn.execute(
                "INSERT INTO reactions(message, sender, emoji, ts) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(message, sender) DO UPDATE SET emoji = excluded.emoji, ts = excluded.ts",
                params![id, sender, emoji, ts],
            )?;
        }
        Ok(Some(id))
    }

    /// Delivery and read receipts on our messages (they only move forward); a read receipt `from_me` means the user
    /// read the chat on another device. Returns the messages that changed.
    pub fn receipt(&self, account: &str, chat: &str, natives: &[String], kind: ReceiptKind, from_me: bool) -> Result<Vec<i64>> {
        let Some(chat_id) = self.find_chat(account, chat)? else { return Ok(Vec::new()) };
        if from_me {
            if kind == ReceiptKind::Read {
                self.conn.execute(
                    "UPDATE chats SET unread = 0, last_read = COALESCE(?2, last_read) WHERE id = ?1",
                    params![chat_id, natives.last()],
                )?;
            }
            return Ok(Vec::new());
        }
        let (state, from) = match kind {
            ReceiptKind::Delivered => ("delivered", "('sent')"),
            ReceiptKind::Read => ("read", "('sent', 'delivered')"),
        };
        let sql = format!(
            "UPDATE messages SET state = ?3 WHERE chat = ?1 AND native_id = ?2 AND from_me = 1 AND state IN {from} RETURNING id"
        );
        let mut changed = Vec::new();
        let mut stmt = self.conn.prepare(&sql)?;
        for native in natives {
            changed.extend(stmt.query_map(params![chat_id, native, state], |r| r.get::<_, i64>(0))?.flatten());
        }
        Ok(changed)
    }

    // -----------------------------------------------------------------------------------------------------------
    // Outbox

    pub fn queue(&self, chat: i64, text: &Text, reply_to: Option<&str>, client_id: &str) -> Result<i64> {
        let spans = (!text.spans.is_empty()).then(|| serde_json::to_string(&text.spans)).transpose()?;
        let now = now_ms();
        self.conn.execute(
            "INSERT INTO messages(chat, client_id, from_me, ts, kind, body, spans, reply_to, state)
             VALUES (?1, ?2, 1, ?3, 'text', ?4, ?5, ?6, 'pending')",
            params![chat, client_id, now, text.plain, spans, reply_to],
        )?;
        let id = self.conn.last_insert_rowid();
        self.conn.execute("UPDATE chats SET last_ts = MAX(last_ts, ?2) WHERE id = ?1", params![chat, now])?;
        Ok(id)
    }

    /// The platform took the message. If its echo already arrived as another row, the pending row gives way.
    pub fn sent(&self, client_id: &str, native: &str, ts: i64) -> Result<Option<i64>> {
        let row: Option<(i64, i64)> = self
            .conn
            .query_row("SELECT id, chat FROM messages WHERE client_id = ?1", [client_id], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?;
        let Some((id, chat)) = row else { return Ok(None) };
        let other: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM messages WHERE chat = ?1 AND native_id = ?2 AND id != ?3",
                params![chat, native, id],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(other) = other {
            self.conn.execute("DELETE FROM messages WHERE id = ?1", [id])?;
            self.conn.execute("UPDATE messages SET client_id = ?2 WHERE id = ?1", params![other, client_id])?;
            return Ok(Some(other));
        }
        self.conn.execute(
            "UPDATE messages SET native_id = ?2, ts = ?3,
                 state = CASE WHEN state IN ('pending', 'failed') THEN 'sent' ELSE state END, error = NULL
             WHERE id = ?1",
            params![id, native, ts],
        )?;
        Ok(Some(id))
    }

    pub fn failed(&self, client_id: &str, error: &str) -> Result<Option<i64>> {
        Ok(self
            .conn
            .query_row(
                "UPDATE messages SET state = 'failed', error = ?2 WHERE client_id = ?1 AND state = 'pending' RETURNING id",
                params![client_id, error],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// Messages still waiting for an account, oldest first.
    pub fn pending(&self, account: &str) -> Result<Vec<Outgoing>> {
        let mut stmt = self.conn.prepare(
            "SELECT m.id, m.client_id, c.native_id, m.body, m.spans, m.reply_to FROM messages m JOIN chats c ON c.id = m.chat
             WHERE c.account = ?1 AND m.state = 'pending' ORDER BY m.id",
        )?;
        let rows = stmt.query_map([account], |r| {
            let spans: Option<String> = r.get(4)?;
            Ok(Outgoing {
                id: r.get(0)?,
                client_id: r.get(1)?,
                chat: r.get(2)?,
                text: Text { plain: r.get(3)?, spans: spans.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default() },
                reply_to: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// A failed message goes back to the outbox.
    pub fn retry(&self, id: i64) -> Result<bool> {
        Ok(self.conn.execute("UPDATE messages SET state = 'pending', error = NULL WHERE id = ?1 AND state = 'failed'", [id])? > 0)
    }

    /// (account, native chat id, native message id) of a message the platform knows.
    pub fn message_ref(&self, id: i64) -> Result<Option<(String, String, String)>> {
        Ok(self
            .conn
            .query_row(
                "SELECT c.account, c.native_id, m.native_id FROM messages m JOIN chats c ON c.id = m.chat
                 WHERE m.id = ?1 AND m.native_id IS NOT NULL",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?)
    }

    // -----------------------------------------------------------------------------------------------------------
    // Reading

    /// The chat is read here: unread drops to zero. Returns the incoming messages that were unread, so the daemon can
    /// acknowledge them on the platform if the receipt policy allows.
    pub fn mark_read(&self, chat: i64) -> Result<Option<ReadMark>> {
        let row: Option<(String, String, String, Option<String>)> = self
            .conn
            .query_row("SELECT account, native_id, kind, last_read FROM chats WHERE id = ?1", [chat], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })
            .optional()?;
        let Some((account, native, kind, last_read)) = row else { return Ok(None) };
        // Everything after the last message read here, (ts, id) ordered so equal timestamps are not lost.
        let (since_ts, since_id): (i64, i64) = match &last_read {
            Some(last) => self
                .conn
                .query_row("SELECT ts, id FROM messages WHERE chat = ?1 AND native_id = ?2", params![chat, last], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })
                .optional()?
                .unwrap_or((i64::MIN, i64::MIN)),
            None => (i64::MIN, i64::MIN),
        };
        let mut stmt = self.conn.prepare(
            "SELECT native_id, sender FROM (
                 SELECT native_id, sender, ts, id FROM messages
                 WHERE chat = ?1 AND from_me = 0 AND deleted = 0 AND native_id IS NOT NULL
                   AND (ts > ?2 OR (ts = ?2 AND id > ?3))
                 ORDER BY ts DESC, id DESC LIMIT 200)
             ORDER BY ts, id",
        )?;
        let messages: Vec<(String, Option<String>)> = stmt
            .query_map(params![chat, since_ts, since_id], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let newest = messages.last().map(|(id, _)| id.clone());
        self.conn
            .execute("UPDATE chats SET unread = 0, last_read = COALESCE(?2, last_read) WHERE id = ?1", params![chat, newest])?;
        Ok(Some(ReadMark { account, chat: native, kind, messages }))
    }

    fn message_view_row(r: &Row) -> rusqlite::Result<Value> {
        let spans: Option<String> = r.get("spans")?;
        let media: Option<String> = r.get("media")?;
        let reactions: Option<String> = r.get("reactions")?;
        Ok(json!({
            "id": r.get::<_, i64>("id")?,
            "chat": r.get::<_, i64>("chat")?,
            "native_id": r.get::<_, Option<String>>("native_id")?,
            "client_id": r.get::<_, Option<String>>("client_id")?,
            "sender": r.get::<_, Option<String>>("sender")?,
            "sender_name": r.get::<_, Option<String>>("sender_name")?,
            "from_me": r.get::<_, bool>("from_me")?,
            "timestamp": r.get::<_, i64>("ts")?,
            "kind": r.get::<_, String>("kind")?,
            "text": {
                "plain": r.get::<_, String>("body")?,
                "spans": spans.and_then(|s| serde_json::from_str::<Value>(&s).ok()).unwrap_or(json!([])),
            },
            "reply_to": r.get::<_, Option<String>>("reply_to")?,
            "media": media.and_then(|s| serde_json::from_str::<Value>(&s).ok()),
            "edited": r.get::<_, Option<i64>>("edited_ts")?.is_some(),
            "deleted": r.get::<_, bool>("deleted")?,
            "state": r.get::<_, String>("state")?,
            "error": r.get::<_, Option<String>>("error")?,
            "reactions": reactions.and_then(|s| serde_json::from_str::<Value>(&s).ok()).unwrap_or(json!([])),
        }))
    }

    fn message_select(filter: &str) -> String {
        format!(
            "SELECT m.*, {SENDER_NAME} AS sender_name,
                 (SELECT json_group_array(json_object('sender', NULLIF(x.sender, ''), 'emoji', x.emoji))
                  FROM reactions x WHERE x.message = m.id) AS reactions
             FROM messages m JOIN chats c ON c.id = m.chat WHERE {filter}"
        )
    }

    pub fn message_view(&self, id: i64) -> Result<Option<Value>> {
        let sql = Store::message_select("m.id = ?1");
        Ok(self.conn.query_row(&sql, [id], Store::message_view_row).optional()?)
    }

    /// The `limit` messages before `before` (a timestamp; none = the newest), oldest first.
    pub fn messages(&self, chat: i64, before: Option<i64>, limit: i64) -> Result<Vec<Value>> {
        let sql = format!(
            "SELECT * FROM ({} ORDER BY m.ts DESC, m.id DESC LIMIT ?3) ORDER BY ts, id",
            Store::message_select("m.chat = ?1 AND (?2 IS NULL OR m.ts < ?2)")
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![chat, before, limit], Store::message_view_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Full-text search across every account, newest first. The query is plain words (no FTS syntax): each word must
    /// appear, the last one as a prefix.
    pub fn search(&self, query: &str, limit: i64) -> Result<Vec<Value>> {
        let Some(fts) = fts_query(query) else { return Ok(Vec::new()) };
        let sql = format!(
            "SELECT m.id, {CHAT_TITLE} AS chat_title, c.account,
                 snippet(messages_fts, 0, '[', ']', '…', 12) AS snippet
             FROM messages_fts f JOIN messages m ON m.id = f.rowid JOIN chats c ON c.id = m.chat
             WHERE messages_fts MATCH ?1 AND m.deleted = 0 ORDER BY m.ts DESC LIMIT ?2"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let hits: Vec<(i64, String, String, String)> = stmt
            .query_map(params![fts, limit], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let mut out = Vec::new();
        for (id, title, account, snippet) in hits {
            if let Some(mut view) = self.message_view(id)? {
                view["chat_title"] = json!(title);
                view["account"] = json!(account);
                view["snippet"] = json!(snippet);
                out.push(view);
            }
        }
        Ok(out)
    }

    // -----------------------------------------------------------------------------------------------------------
    // Totals for the bar

    /// Unread messages in chats that are not muted.
    pub fn unread_total(&self) -> Result<i64> {
        let sql = format!("SELECT COALESCE(SUM(c.unread), 0) FROM chats c WHERE {NOT_MUTED}");
        Ok(self.conn.query_row(&sql, rusqlite::named_params! {":now": now_ms()}, |r| r.get(0))?)
    }

    /// (title, line) of the newest unread incoming messages, for the bar's tooltip: the chat's title, and the text
    /// (prefixed by the sender in groups).
    pub fn recent_unread(&self, limit: i64) -> Result<Vec<(String, String)>> {
        let sql = format!(
            "SELECT {CHAT_TITLE}, c.kind, {SENDER_NAME}, m.sender, m.kind, m.body, m.media FROM (
                 SELECT *, ROW_NUMBER() OVER (PARTITION BY chat ORDER BY ts DESC, id DESC) AS rank
                 FROM messages WHERE from_me = 0) m
             JOIN chats c ON c.id = m.chat
             WHERE c.unread > 0 AND {NOT_MUTED} AND m.deleted = 0 AND m.rank <= c.unread
             ORDER BY m.ts DESC, m.id DESC LIMIT :limit"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::named_params! {":now": now_ms(), ":limit": limit}, |r| {
            let title: String = r.get(0)?;
            let chat_kind: String = r.get(1)?;
            let sender: Option<String> = r.get::<_, Option<String>>(2)?.or(r.get(3)?);
            let kind: String = r.get(4)?;
            let body: String = r.get(5)?;
            let media: Option<String> = r.get(6)?;
            let line = preview(&kind, &body, media.as_deref());
            let line = match (chat_kind.as_str(), sender) {
                ("direct", _) | (_, None) => line,
                (_, Some(sender)) => format!("{sender}: {line}"),
            };
            Ok((title, line))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

/// One line for a message in a list or a notification: its text, or its kind and file name.
pub fn preview(kind: &str, body: &str, media: Option<&str>) -> String {
    if !body.is_empty() {
        return body.to_string();
    }
    let name = media
        .and_then(|m| serde_json::from_str::<Value>(m).ok())
        .and_then(|m| m.get("name").and_then(Value::as_str).map(str::to_owned));
    let label = match kind {
        "image" => "Photo",
        "video" => "Video",
        "audio" => "Audio",
        "voice" => "Voice message",
        "file" => "File",
        "sticker" => "Sticker",
        "location" => "Location",
        "contact" => "Contact",
        "poll" => "Poll",
        "text" => "",
        _ => "Unsupported message",
    };
    match name {
        Some(name) if !label.is_empty() => format!("{label}: {name}"),
        _ => label.to_string(),
    }
}

/// Plain words → an FTS5 query: every word quoted (so no FTS syntax gets through), the last one a prefix.
fn fts_query(query: &str) -> Option<String> {
    let words: Vec<String> =
        query.split_whitespace().map(|w| w.replace('"', "")).filter(|w| !w.is_empty()).map(|w| format!("\"{w}\"")).collect();
    let (last, rest) = words.split_last()?;
    let mut out = rest.join(" ");
    if !out.is_empty() {
        out.push(' ');
    }
    out.push_str(last);
    out.push('*');
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Text;

    fn msg(id: &str, chat: &str, sender: Option<&str>, ts: i64, text: &str) -> Message {
        Message {
            id: id.into(),
            chat: chat.into(),
            sender: sender.map(Into::into),
            from_me: sender.is_none(),
            timestamp: ts,
            kind: "text".into(),
            text: Some(Text::plain(text)),
            reply_to: None,
            media: None,
            client_id: None,
        }
    }

    fn store_with_account() -> (Store, String) {
        let store = Store::open_in_memory().unwrap();
        let key = store.add_account("fake").unwrap();
        (store, key)
    }

    #[test]
    fn account_keys_count_per_platform() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(store.add_account("whatsapp").unwrap(), "whatsapp-1");
        assert_eq!(store.add_account("whatsapp").unwrap(), "whatsapp-2");
        assert_eq!(store.add_account("telegram").unwrap(), "telegram-1");
        store.remove_account("whatsapp-1").unwrap();
        assert_eq!(store.add_account("whatsapp").unwrap(), "whatsapp-3");
    }

    #[test]
    fn live_messages_count_as_unread_history_does_not() {
        let (store, key) = store_with_account();
        store.put_history(&key, &[msg("a", "paul", Some("paul"), 1, "old")]).unwrap();
        let first = store.put_message(&key, &msg("b", "paul", Some("paul"), 2, "new"), true).unwrap();
        assert!(first.new);
        let again = store.put_message(&key, &msg("b", "paul", Some("paul"), 2, "new"), true).unwrap();
        assert!(!again.new);
        assert_eq!(again.id, first.id);
        store.put_message(&key, &msg("c", "paul", None, 3, "mine"), true).unwrap();
        assert_eq!(store.unread_total().unwrap(), 1);
        let chat = store.chats(None, false, 10).unwrap();
        assert_eq!(chat[0]["last"]["text"]["plain"], "mine");
    }

    #[test]
    fn titles_fall_back_to_the_contact_then_the_id() {
        let (store, key) = store_with_account();
        store.put_message(&key, &msg("a", "paul", Some("paul"), 1, "hi"), true).unwrap();
        store.put_message(&key, &msg("b", "x", Some("x"), 2, "hi"), true).unwrap();
        store.upsert_contact(&key, &Contact { id: "paul".into(), name: Some("Paul".into()), phone: None }).unwrap();
        let titles: Vec<String> =
            store.chats(None, false, 10).unwrap().iter().map(|c| c["title"].as_str().unwrap().to_string()).collect();
        assert_eq!(titles, vec!["x", "Paul"]);
        assert_eq!(store.recent_unread(5).unwrap()[1], ("Paul".to_string(), "hi".to_string()));
    }

    #[test]
    fn groups_prefix_the_sender_and_muted_chats_do_not_count() {
        let (store, key) = store_with_account();
        store
            .upsert_chat(
                &key,
                &Chat { id: "fam".into(), kind: Some("group".into()), title: Some("Family".into()), ..Default::default() },
            )
            .unwrap();
        store.upsert_contact(&key, &Contact { id: "lea".into(), name: Some("Léa".into()), phone: None }).unwrap();
        store.put_message(&key, &msg("a", "fam", Some("lea"), 1, "dinner?"), true).unwrap();
        assert_eq!(store.recent_unread(5).unwrap(), vec![("Family".to_string(), "Léa: dinner?".to_string())]);
        store.upsert_chat(&key, &Chat { id: "fam".into(), muted_until: Some(-1), ..Default::default() }).unwrap();
        assert_eq!(store.unread_total().unwrap(), 0);
        assert!(store.recent_unread(5).unwrap().is_empty());
        store.upsert_chat(&key, &Chat { id: "fam".into(), muted_until: Some(1), ..Default::default() }).unwrap();
        assert_eq!(store.unread_total().unwrap(), 1, "a mute that ended counts again");
    }

    #[test]
    fn outbox_goes_pending_sent_then_read() {
        let (store, key) = store_with_account();
        let chat = store.upsert_chat(&key, &Chat { id: "paul".into(), ..Default::default() }).unwrap();
        let id = store.queue(chat, &Text::plain("hello"), None, "c1").unwrap();
        assert_eq!(store.pending(&key).unwrap().len(), 1);
        assert_eq!(store.sent("c1", "n1", 5).unwrap(), Some(id));
        assert!(store.pending(&key).unwrap().is_empty());
        store.receipt(&key, "paul", &["n1".into()], ReceiptKind::Read, false).unwrap();
        assert_eq!(store.receipt(&key, "paul", &["n1".into()], ReceiptKind::Delivered, false).unwrap(), Vec::<i64>::new());
        assert_eq!(store.message_view(id).unwrap().unwrap()["state"], "read", "receipts never go backwards");
    }

    #[test]
    fn an_echo_before_the_reply_does_not_duplicate() {
        let (store, key) = store_with_account();
        let chat = store.upsert_chat(&key, &Chat { id: "paul".into(), ..Default::default() }).unwrap();
        store.queue(chat, &Text::plain("hello"), None, "c1").unwrap();
        // Echo without client_id (another code path), then the reply.
        store.put_message(&key, &msg("n1", "paul", None, 5, "hello"), true).unwrap();
        let kept = store.sent("c1", "n1", 5).unwrap().unwrap();
        assert_eq!(store.messages(chat, None, 10).unwrap().len(), 1);
        assert_eq!(store.message_view(kept).unwrap().unwrap()["client_id"], "c1");
        // Echo with client_id: matched to the pending row.
        store.queue(chat, &Text::plain("again"), None, "c2").unwrap();
        let mut echo = msg("n2", "paul", None, 6, "again");
        echo.client_id = Some("c2".into());
        store.put_message(&key, &echo, true).unwrap();
        assert_eq!(store.messages(chat, None, 10).unwrap().len(), 2);
        assert!(store.pending(&key).unwrap().is_empty());
    }

    #[test]
    fn mark_read_returns_what_to_acknowledge() {
        let (store, key) = store_with_account();
        for (i, id) in ["a", "b", "c"].iter().enumerate() {
            store.put_message(&key, &msg(id, "paul", Some("paul"), i as i64, "x"), true).unwrap();
        }
        let chat = store.chats(None, false, 1).unwrap()[0]["id"].as_i64().unwrap();
        let mark = store.mark_read(chat).unwrap().unwrap();
        assert_eq!(mark.messages.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(), vec!["a", "b", "c"]);
        assert_eq!(store.unread_total().unwrap(), 0);
        store.put_message(&key, &msg("d", "paul", Some("paul"), 9, "y"), true).unwrap();
        let mark = store.mark_read(chat).unwrap().unwrap();
        assert_eq!(mark.messages.len(), 1, "only what came after the last read");
    }

    #[test]
    fn edits_deletes_reactions_and_search() {
        let (store, key) = store_with_account();
        store.put_message(&key, &msg("a", "paul", Some("paul"), 1, "On mange au café ?"), true).unwrap();
        store.put_message(&key, &msg("b", "paul", Some("paul"), 2, "secret"), true).unwrap();
        assert_eq!(store.search("cafe", 10).unwrap().len(), 1, "accents ignored, prefix match");
        assert_eq!(store.search("mange caf", 10).unwrap().len(), 1);
        assert!(store.search("\"; DROP", 10).unwrap().is_empty());
        assert!(store.search("   ", 10).unwrap().is_empty());
        store.edit_message(&key, "paul", "a", &Text::plain("On mange à la cantine"), None).unwrap();
        assert!(store.search("cafe", 10).unwrap().is_empty());
        assert_eq!(store.search("cantine", 10).unwrap()[0]["edited"], true);
        let id = store.react(&key, "paul", "b", None, "👍", None).unwrap().unwrap();
        store.react(&key, "paul", "b", Some("paul"), "❤️", None).unwrap();
        assert_eq!(store.message_view(id).unwrap().unwrap()["reactions"].as_array().unwrap().len(), 2);
        store.react(&key, "paul", "b", None, "", None).unwrap();
        assert_eq!(store.message_view(id).unwrap().unwrap()["reactions"].as_array().unwrap().len(), 1);
        store.delete_message(&key, "paul", "b").unwrap();
        let view = store.message_view(id).unwrap().unwrap();
        assert_eq!(view["deleted"], true);
        assert_eq!(view["text"]["plain"], "");
        assert!(store.search("secret", 10).unwrap().is_empty());
    }

    #[test]
    fn removing_an_account_removes_its_messages() {
        let (store, key) = store_with_account();
        store.put_message(&key, &msg("a", "paul", Some("paul"), 1, "x"), true).unwrap();
        store.remove_account(&key).unwrap();
        assert!(store.chats(None, false, 10).unwrap().is_empty());
        assert!(store.search("x", 10).unwrap().is_empty());
    }

    #[test]
    fn previews_name_the_media() {
        assert_eq!(preview("text", "hi", None), "hi");
        assert_eq!(preview("image", "", Some(r#"{"kind":"image","name":"a.jpg"}"#)), "Photo: a.jpg");
        assert_eq!(preview("voice", "", None), "Voice message");
        assert_eq!(preview("hologram", "", None), "Unsupported message");
    }

    #[test]
    fn reopening_keeps_the_data() {
        let dir = std::env::temp_dir().join(format!("fili-im-store-{}", std::process::id()));
        let path = dir.join("c.db");
        {
            let store = Store::open(&path).unwrap();
            let key = store.add_account("fake").unwrap();
            store.put_message(&key, &msg("a", "paul", Some("paul"), 1, "x"), true).unwrap();
        }
        let store = Store::open(&path).unwrap();
        assert_eq!(store.unread_total().unwrap(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
