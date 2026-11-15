//! The control socket (`$XDG_RUNTIME_DIR/fili-im/daemon.sock`): JSON lines for the window and the CLI.
//!
//! Requests `{"id": 1, "method": "chats.list", "params": {…}}` get `{"reply": 1, "ok": true, "result": …}` or
//! `{"reply": 1, "ok": false, "error": {"code", "message"}}`. After `subscribe`, the daemon also pushes events
//! (`{"event": "message", …}`); a client that falls behind gets `{"event": "resync"}` and reloads its lists.
use super::Daemon;
use crate::paths;
use crate::protocol::{ErrorBody, Text};
use serde_json::{Map, Value, json};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, mpsc};

pub fn listen() -> std::io::Result<UnixListener> {
    use std::os::unix::fs::PermissionsExt;
    let path = paths::socket();
    // The instance lock is held: a socket left here belongs to a daemon that is gone.
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

pub async fn accept(daemon: Arc<Daemon>, listener: UnixListener) {
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                tokio::spawn(client(daemon.clone(), stream));
            }
            Err(e) => {
                eprintln!("fili-im: control socket: {e}");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

async fn client(daemon: Arc<Daemon>, stream: UnixStream) {
    let (read, mut write) = stream.into_split();
    let (out, mut rx) = mpsc::channel::<String>(256);
    let writer = tokio::spawn(async move {
        while let Some(mut line) = rx.recv().await {
            line.push('\n');
            if write.write_all(line.as_bytes()).await.is_err() {
                break;
            }
        }
    });
    let mut forwarder: Option<tokio::task::JoinHandle<()>> = None;
    let mut lines = BufReader::new(read).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let request: Value = match serde_json::from_str(&line) {
            Ok(Value::Object(map)) => Value::Object(map),
            _ => {
                let _ = out
                    .send(
                        json!({"reply": null, "ok": false, "error": {"code": "invalid", "message": "not a JSON object"}})
                            .to_string(),
                    )
                    .await;
                continue;
            }
        };
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let method = request.get("method").and_then(Value::as_str).unwrap_or("").to_string();
        let params = request.get("params").and_then(Value::as_object).cloned().unwrap_or_default();
        let answer = if method == "subscribe" {
            if forwarder.is_none() {
                forwarder = Some(tokio::spawn(forward(daemon.events.subscribe(), out.clone())));
            }
            Ok(json!({}))
        } else {
            call(&daemon, &method, &params).await
        };
        let reply = match answer {
            Ok(result) => json!({"reply": id, "ok": true, "result": result}),
            Err(e) => json!({"reply": id, "ok": false, "error": e}),
        };
        if out.send(reply.to_string()).await.is_err() {
            break;
        }
    }
    if let Some(forwarder) = forwarder {
        forwarder.abort();
    }
    drop(out);
    let _ = writer.await;
}

async fn forward(mut events: broadcast::Receiver<Value>, out: mpsc::Sender<String>) {
    loop {
        let line = match events.recv().await {
            Ok(event) => event.to_string(),
            Err(broadcast::error::RecvError::Lagged(_)) => json!({"event": "resync"}).to_string(),
            Err(broadcast::error::RecvError::Closed) => break,
        };
        if out.send(line).await.is_err() {
            break;
        }
    }
}

fn invalid(message: &str) -> ErrorBody {
    ErrorBody::new("invalid", message)
}

fn str_param<'a>(params: &'a Map<String, Value>, name: &str) -> Result<&'a str, ErrorBody> {
    params.get(name).and_then(Value::as_str).filter(|s| !s.is_empty()).ok_or_else(|| invalid(&format!("missing {name}")))
}

fn int_param(params: &Map<String, Value>, name: &str) -> Result<i64, ErrorBody> {
    params.get(name).and_then(Value::as_i64).ok_or_else(|| invalid(&format!("missing {name}")))
}

fn limit(params: &Map<String, Value>, default: i64) -> i64 {
    params.get("limit").and_then(Value::as_i64).unwrap_or(default).clamp(1, 1000)
}

fn bool_param(params: &Map<String, Value>, name: &str) -> bool {
    params.get(name).and_then(Value::as_bool).unwrap_or(false)
}

/// `text` is a string or a neutral text object.
fn text_param(params: &Map<String, Value>) -> Result<Text, ErrorBody> {
    match params.get("text") {
        Some(Value::String(s)) => Ok(Text::plain(s.clone())),
        Some(value @ Value::Object(_)) => serde_json::from_value(value.clone()).map_err(|e| invalid(&format!("text: {e}"))),
        _ => Err(invalid("missing text")),
    }
}

fn internal(e: anyhow::Error) -> ErrorBody {
    super::internal(e)
}

async fn call(daemon: &Arc<Daemon>, method: &str, params: &Map<String, Value>) -> Result<Value, ErrorBody> {
    let store = || daemon.store.lock().unwrap();
    match method {
        "hello" => Ok(json!({"name": "fili-im", "version": env!("CARGO_PKG_VERSION"), "api": super::API})),
        "status" => {
            let store = store();
            Ok(json!({"unread": store.unread_total().map_err(internal)?, "accounts": store.accounts().map_err(internal)?}))
        }
        "platforms.list" => Ok(Value::Array(
            paths::connectors()
                .into_iter()
                .map(|(platform, program)| json!({"platform": platform, "program": program.display().to_string()}))
                .collect(),
        )),
        "accounts.list" => Ok(Value::Array(store().accounts().map_err(internal)?)),
        "account.add" => {
            let platform = str_param(params, "platform")?;
            if paths::find_connector(platform).is_none() {
                return Err(ErrorBody::new("not_found", format!("no connector for {platform:?} (fili-im-{platform})")));
            }
            let key = store().add_account(platform).map_err(internal)?;
            daemon.start(&key, platform);
            daemon.account_changed(&key);
            Ok(store().account(&key).map_err(internal)?.unwrap_or(Value::Null))
        }
        "account.pair" => {
            let key = str_param(params, "account")?;
            let conn = daemon.connector(key).ok_or_else(|| ErrorBody::new("not_found", "no such account"))?;
            let method = match params.get("method").and_then(Value::as_str) {
                Some(m) => m.to_string(),
                None => store().capabilities(key).map_err(internal)?.pairing.first().cloned().unwrap_or_default(),
            };
            let mut args = Map::new();
            args.insert("method".into(), json!(method));
            if let Some(phone) = params.get("phone") {
                args.insert("phone".into(), phone.clone());
            }
            conn.command("pair", args, Duration::from_secs(30)).await
        }
        "account.remove" => {
            let key = str_param(params, "account")?.to_string();
            if store().account(&key).map_err(internal)?.is_none() {
                return Err(ErrorBody::new("not_found", "no such account"));
            }
            if let Some(conn) = daemon.connector(&key) {
                // Forget the session on the platform too (the linked device disappears from the phone).
                let _ = conn.command("logout", Map::new(), Duration::from_secs(10)).await;
            }
            daemon.stop(&key).await;
            store().remove_account(&key).map_err(internal)?;
            for dir in [paths::account_dir(&key), paths::media_dir(&key)] {
                let _ = std::fs::remove_dir_all(dir);
            }
            daemon.broadcast(json!({"event": "account.removed", "account": key}));
            daemon.refresh_alerts();
            Ok(json!({}))
        }
        "chats.list" => {
            let account = params.get("account").and_then(Value::as_str);
            Ok(Value::Array(store().chats(account, bool_param(params, "unread"), limit(params, 200)).map_err(internal)?))
        }
        "chat.get" => store()
            .chat(int_param(params, "chat")?)
            .map_err(internal)?
            .ok_or_else(|| ErrorBody::new("not_found", "no such conversation")),
        "messages.list" => {
            let chat = int_param(params, "chat")?;
            let before = params.get("before").and_then(Value::as_i64);
            Ok(Value::Array(store().messages(chat, before, limit(params, 50)).map_err(internal)?))
        }
        "send" => {
            let chat = int_param(params, "chat")?;
            let reply_to = params.get("reply_to").and_then(Value::as_str).map(str::to_owned);
            daemon.send(chat, text_param(params)?, reply_to)
        }
        "message.retry" => {
            let id = int_param(params, "message")?;
            if !store().retry(id).map_err(internal)? {
                return Err(ErrorBody::new("invalid", "only a failed message can be sent again"));
            }
            daemon.message_changed(id);
            let chat = store().message_view(id).map_err(internal)?.and_then(|v| v["chat"].as_i64());
            let account = chat.and_then(|chat| store().chat_ref(chat).ok().flatten()).map(|(account, _, _)| account);
            if let Some(conn) = account.and_then(|account| daemon.connector(&account)) {
                tokio::spawn(super::flush(daemon.clone(), conn));
            }
            Ok(json!({}))
        }
        "mark_read" => mark_read(daemon, int_param(params, "chat")?, bool_param(params, "receipt")),
        "react" | "edit" | "delete" => act(daemon, method, params).await,
        "search" => Ok(Value::Array(store().search(str_param(params, "query")?, limit(params, 50)).map_err(internal)?)),
        "shutdown" => {
            daemon.quit.notify_one();
            Ok(json!({}))
        }
        _ => Err(ErrorBody::new("unsupported", format!("unknown method {method:?}"))),
    }
}

/// Read here; with `receipt`, also tell the platform (only where it can, and only when asked: the receipt policy of
/// phase P3 decides when the window asks).
fn mark_read(daemon: &Arc<Daemon>, chat: i64, receipt: bool) -> Result<Value, ErrorBody> {
    let mark = daemon.store.lock().unwrap().mark_read(chat).map_err(internal)?;
    let Some(mark) = mark else { return Err(ErrorBody::new("not_found", "no such conversation")) };
    daemon.chat_changed(chat);
    daemon.refresh_alerts();
    let caps = daemon.store.lock().unwrap().capabilities(&mark.account).map_err(internal)?;
    let acknowledged = receipt && caps.receipts.send && !mark.messages.is_empty();
    if acknowledged && let Some(conn) = daemon.connector(&mark.account) {
        // Group chats acknowledge per sender (WhatsApp does); a direct chat has one.
        let mut by_sender: Vec<(Option<String>, Vec<String>)> = Vec::new();
        for (id, sender) in mark.messages.clone() {
            match by_sender.iter_mut().find(|(s, _)| *s == sender) {
                Some((_, ids)) => ids.push(id),
                None => by_sender.push((sender, vec![id])),
            }
        }
        let group = mark.kind != "direct";
        let native_chat = mark.chat.clone();
        tokio::spawn(async move {
            for (sender, ids) in by_sender {
                let mut args = Map::new();
                args.insert("chat".into(), json!(native_chat));
                args.insert("messages".into(), json!(ids));
                if group && let Some(sender) = sender {
                    args.insert("sender".into(), json!(sender));
                }
                if let Err(e) = conn.command("mark_read", args, Duration::from_secs(30)).await {
                    eprintln!("fili-im: {}: mark_read: {}", conn.key, e.message);
                }
            }
        });
    }
    Ok(json!({"read": mark.messages.len(), "receipt": acknowledged}))
}

/// React, edit or delete a message the platform knows, then apply it here.
async fn act(daemon: &Arc<Daemon>, method: &str, params: &Map<String, Value>) -> Result<Value, ErrorBody> {
    let id = int_param(params, "message")?;
    let found = daemon.store.lock().unwrap().message_ref(id).map_err(internal)?;
    let Some((account, chat, native)) = found else {
        return Err(ErrorBody::new("not_found", "no such message (or not sent yet)"));
    };
    let conn = daemon.connector(&account).ok_or_else(|| ErrorBody::new("not_connected", "the account is not running"))?;
    let mut args = Map::new();
    args.insert("chat".into(), json!(chat));
    let text = if method == "edit" { Some(text_param(params)?) } else { None };
    let emoji = params.get("emoji").and_then(Value::as_str).unwrap_or("").to_string();
    match method {
        "react" => {
            args.insert("message".into(), json!(native));
            args.insert("emoji".into(), json!(emoji));
        }
        "edit" => {
            args.insert("message".into(), json!(native));
            args.insert("text".into(), serde_json::to_value(&text).unwrap_or(Value::Null));
        }
        _ => {
            args.insert("message".into(), json!(native));
            args.insert("for_everyone".into(), json!(bool_param(params, "for_everyone")));
        }
    }
    conn.command(method, args, Duration::from_secs(30)).await?;
    {
        let store = daemon.store.lock().unwrap();
        let applied = match (method, text) {
            ("react", _) => store.react(&account, &chat, &native, None, &emoji, Some(crate::store::now_ms())),
            ("edit", Some(text)) => store.edit_message(&account, &chat, &native, &text, None),
            _ => store.delete_message(&account, &chat, &native),
        };
        applied.map_err(internal)?;
    }
    daemon.message_changed(id);
    Ok(json!({}))
}
