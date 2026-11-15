//! The control socket (`$XDG_RUNTIME_DIR/fili-im/daemon.sock`): JSON lines for the window and the CLI.
//!
//! Requests `{"id": 1, "method": "chats.list", "params": {…}}` get `{"reply": 1, "ok": true, "result": …}` or
//! `{"reply": 1, "ok": false, "error": {"code", "message"}}`. After `subscribe`, the daemon also pushes events
//! (`{"event": "message", …}`); a client that falls behind gets `{"event": "resync"}` and reloads its lists.
use super::Daemon;
use crate::paths;
use crate::policy::Ask;
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
        } else if method == "media.fetch" {
            // A download can take minutes: answered from its own task so the connection keeps serving other requests.
            let (daemon, out) = (daemon.clone(), out.clone());
            tokio::spawn(async move {
                let reply = match fetch_media(&daemon, &params).await {
                    Ok(result) => json!({"reply": id, "ok": true, "result": result}),
                    Err(e) => json!({"reply": id, "ok": false, "error": e}),
                };
                let _ = out.send(reply.to_string()).await;
            });
            continue;
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
            let text = if params.contains_key("text") { text_param(params)? } else { Text::plain("") };
            let media = match params.get("media") {
                Some(Value::Object(m)) => Some(outgoing_media(daemon, chat, m)?),
                _ => None,
            };
            daemon.send(chat, text, reply_to, media)
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
        "mark_read" => {
            // `receipt`: true = the user said "mark as read", "auto" = the conversation was opened (the policy decides),
            // absent or false = read here only (a peek)
            let ask = match params.get("receipt") {
                Some(Value::Bool(true)) => Ask::Explicit,
                Some(Value::String(s)) if s == "auto" => Ask::Auto,
                _ => Ask::No,
            };
            mark_read(daemon, int_param(params, "chat")?, ask)
        }
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
pub(super) fn mark_read(daemon: &Arc<Daemon>, chat: i64, ask: Ask) -> Result<Value, ErrorBody> {
    let mark = daemon.store.lock().unwrap().mark_read(chat).map_err(internal)?;
    let Some(mark) = mark else { return Err(ErrorBody::new("not_found", "no such conversation")) };
    daemon.chat_changed(chat);
    daemon.refresh_alerts();
    let caps = daemon.store.lock().unwrap().capabilities(&mark.account).map_err(internal)?;
    let acknowledged = crate::policy::receipts(&mark.account, chat).sends(ask) && caps.receipts.send && !mark.messages.is_empty();
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
    if method == "delete" && bool_param(params, "local") {
        let removed = daemon.store.lock().unwrap().forget_message(id).map_err(internal)?;
        return if removed { Ok(json!({})) } else { Err(ErrorBody::new("not_found", "no such message")) };
    }
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

/// MIME type from a file name, for the few kinds the window offers (the connector sniffs the content when it is empty).
fn mime_of(name: &str) -> Option<&'static str> {
    let ext = name.rsplit_once('.')?.1.to_ascii_lowercase();
    Some(match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        "mkv" => "video/x-matroska",
        "pdf" => "application/pdf",
        _ => return None,
    })
}

/// A file the user picked, as an outgoing media: copied next to the account's media (so a retry still finds it after the
/// original moves), kind from its type.
fn outgoing_media(daemon: &Arc<Daemon>, chat: i64, params: &Map<String, Value>) -> Result<crate::protocol::Media, ErrorBody> {
    let source = std::path::PathBuf::from(str_param(params, "path")?);
    let meta = std::fs::metadata(&source).map_err(|e| ErrorBody::new("invalid", format!("cannot read the file: {e}")))?;
    if !meta.is_file() {
        return Err(invalid("not a file"));
    }
    let name = source.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "file".into());
    let mime = params.get("mime").and_then(Value::as_str).map(str::to_owned).or_else(|| mime_of(&name).map(str::to_owned));
    let kind = match mime.as_deref() {
        Some(m) if m.starts_with("image/") && m != "image/gif" => "image",
        Some(m) if m.starts_with("video/") => "video",
        _ => "file",
    };
    let account = daemon.store.lock().unwrap().chat_ref(chat).map_err(internal)?.map(|(a, _, _)| a);
    let account = account.ok_or_else(|| ErrorBody::new("not_found", "no such conversation"))?;
    let dir = paths::media_dir(&account).join("out");
    paths::private_dir(&dir).map_err(|e| ErrorBody::new("failed", format!("cannot create {}: {e}", dir.display())))?;
    let stored = dir.join(format!("{}-{}", crate::store::now_ms(), name.replace('/', "_")));
    std::fs::copy(&source, &stored).map_err(|e| ErrorBody::new("failed", format!("cannot copy the file: {e}")))?;
    Ok(crate::protocol::Media {
        kind: kind.into(),
        mime,
        name: Some(name),
        size: Some(meta.len()),
        reference: String::new(),
        path: Some(stored.display().to_string()),
    })
}

/// `media.fetch {message}`: the local file of a message's media, downloaded through the connector the first time.
async fn fetch_media(daemon: &Arc<Daemon>, params: &Map<String, Value>) -> Result<Value, ErrorBody> {
    let mut result = fetch_file(daemon, params).await?;
    // A video gets a still picture (first seconds) for the bubble; mpv draws it, nothing is done without mpv
    let id = int_param(params, "message")?;
    let is_video = daemon
        .store
        .lock()
        .unwrap()
        .message_view(id)
        .ok()
        .flatten()
        .and_then(|v| v.get("media").and_then(|m| m.get("kind")).and_then(Value::as_str).map(|k| k == "video"))
        .unwrap_or(false);
    if is_video
        && let Some(path) = result.get("path").and_then(Value::as_str)
        && let Some(thumb) = video_thumb(path).await
    {
        result["thumb"] = json!(thumb);
    }
    Ok(result)
}

/// `<video>.thumb.jpg`, made once with mpv (first frame after a short seek).
async fn video_thumb(video: &str) -> Option<String> {
    let thumb = format!("{video}.thumb.jpg");
    if std::path::Path::new(&thumb).is_file() {
        return Some(thumb);
    }
    let dir = format!("{video}.thumb.d");
    std::fs::create_dir_all(&dir).ok()?;
    let run = tokio::process::Command::new("mpv")
        .args(["--no-config", "--no-audio", "--really-quiet", "--frames=1", "--start=0.5", "--vo=image", "--vo-image-format=jpg"])
        .arg(format!("--vo-image-outdir={dir}"))
        .arg(video)
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true)
        .status();
    let _ = tokio::time::timeout(Duration::from_secs(20), run).await;
    let made = std::fs::read_dir(&dir).ok()?.flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|e| e == "jpg"));
    let ok = made.and_then(|f| std::fs::rename(f, &thumb).ok()).is_some();
    let _ = std::fs::remove_dir_all(&dir);
    ok.then_some(thumb)
}

async fn fetch_file(daemon: &Arc<Daemon>, params: &Map<String, Value>) -> Result<Value, ErrorBody> {
    let id = int_param(params, "message")?;
    let (view, found) = {
        let store = daemon.store.lock().unwrap();
        (store.message_view(id).map_err(internal)?, store.message_ref(id).map_err(internal)?)
    };
    let media = view.as_ref().and_then(|v| v.get("media")).filter(|m| m.is_object()).cloned();
    let Some(media) = media else { return Err(ErrorBody::new("not_found", "this message has no file")) };
    // Sent from here: the copy made for sending is the file
    if let Some(path) = media.get("path").and_then(Value::as_str)
        && std::path::Path::new(path).is_file()
    {
        return Ok(json!({"path": path}));
    }
    let Some((account, chat, native)) = found else { return Err(ErrorBody::new("not_found", "no such message")) };
    let name = media.get("name").and_then(Value::as_str).unwrap_or("");
    let ext = std::path::Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().into_owned())
        .or_else(|| match media.get("mime").and_then(Value::as_str) {
            Some("image/jpeg") => Some("jpg".into()),
            Some("image/png") => Some("png".into()),
            Some("image/webp") => Some("webp".into()),
            Some("video/mp4") => Some("mp4".into()),
            Some("audio/ogg") | Some("audio/ogg; codecs=opus") => Some("ogg".into()),
            Some("application/pdf") => Some("pdf".into()),
            _ => None,
        })
        .unwrap_or_else(|| "bin".into());
    let dir = paths::media_dir(&account).join("in");
    paths::private_dir(&dir).map_err(|e| ErrorBody::new("failed", format!("cannot create {}: {e}", dir.display())))?;
    let stem: String = if name.is_empty() { format!("{id}") } else { format!("{id}-{}", name.replace('/', "_")) };
    let dest = if name.is_empty() { dir.join(format!("{stem}.{ext}")) } else { dir.join(stem) };
    if dest.is_file() {
        return Ok(json!({"path": dest.display().to_string()}));
    }
    let conn = daemon.connector(&account).ok_or_else(|| ErrorBody::new("not_connected", "the account is not running"))?;
    let mut args = Map::new();
    args.insert("chat".into(), json!(chat));
    args.insert("message".into(), json!(native));
    args.insert("ref".into(), media.get("ref").cloned().unwrap_or(json!(native)));
    args.insert("dest".into(), json!(dest.display().to_string()));
    conn.command("fetch_media", args, Duration::from_secs(300)).await?;
    Ok(json!({"path": dest.display().to_string()}))
}
