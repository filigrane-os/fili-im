//! `fili-im daemon`: one per session. Owns the store and the connectors, notifies of new messages, keeps the bar's
//! alert files, and serves the window and the CLI on its control socket.
mod connector;
mod control;
mod notify;

use crate::paths;
use crate::protocol::{ConnState, Event, Text};
use crate::store::{Outgoing, Store, now_ms, preview};
use connector::Connector;
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

pub const API: u32 = 1;
const SEND_TIMEOUT: Duration = Duration::from_secs(60);

struct Running {
    conn: Arc<Connector>,
    task: JoinHandle<()>,
}

pub struct Daemon {
    store: Mutex<Store>,
    connectors: Mutex<HashMap<String, Running>>,
    events: broadcast::Sender<Value>,
    notifier: notify::Notifier,
    alerts: notify::Alerts,
    quit: tokio::sync::Notify,
    debug: bool,
}

/// One daemon per session: a second one (niri restarted, a manual run) exits quietly. The lock lives as long as the
/// returned file is open.
fn single_instance() -> Option<std::fs::File> {
    use std::os::fd::AsRawFd;
    let dir = paths::runtime_dir();
    paths::private_dir(&dir).ok()?;
    let file = std::fs::OpenOptions::new().create(true).write(true).truncate(false).open(dir.join("daemon.lock")).ok()?;
    // SAFETY: flock on a descriptor this function owns.
    let locked = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0;
    locked.then_some(file)
}

pub fn run() -> i32 {
    let Some(_lock) = single_instance() else {
        eprintln!("fili-im: the daemon is already running");
        return 0;
    };
    let store = match Store::open(&paths::database()) {
        Ok(store) => store,
        Err(e) => {
            eprintln!("fili-im: {e:#}");
            return 1;
        }
    };
    let runtime = match tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build() {
        Ok(runtime) => runtime,
        Err(e) => {
            eprintln!("fili-im: cannot start the runtime: {e}");
            return 1;
        }
    };
    runtime.block_on(async move {
        let daemon = Arc::new(Daemon {
            store: Mutex::new(store),
            connectors: Mutex::new(HashMap::new()),
            events: broadcast::channel(1024).0,
            notifier: notify::Notifier::new(std::env::var_os("FILI_IM_NO_NOTIFY").is_none()),
            alerts: notify::Alerts::default(),
            quit: tokio::sync::Notify::new(),
            debug: std::env::var_os("FILI_IM_DEBUG").is_some(),
        });
        daemon.serve().await
    })
}

impl Daemon {
    async fn serve(self: &Arc<Self>) -> i32 {
        let listener = match control::listen() {
            Ok(listener) => listener,
            Err(e) => {
                eprintln!("fili-im: cannot open the control socket: {e}");
                return 1;
            }
        };
        let accounts = self.store.lock().unwrap().account_keys().unwrap_or_default();
        for (key, platform) in accounts {
            self.start(&key, &platform);
        }
        self.refresh_alerts();
        let server = tokio::spawn(control::accept(self.clone(), listener));
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).ok();
        tokio::select! {
            _ = self.quit.notified() => {}
            _ = tokio::signal::ctrl_c() => {}
            _ = async { match term.as_mut() { Some(t) => { t.recv().await; } None => std::future::pending().await } } => {}
        }
        server.abort();
        let _ = std::fs::remove_file(paths::socket());
        let running: Vec<Running> = self.connectors.lock().unwrap().drain().map(|(_, r)| r).collect();
        for r in &running {
            r.conn.stop();
        }
        for r in running {
            let _ = tokio::time::timeout(Duration::from_secs(5), r.task).await;
        }
        0
    }

    pub fn debug(&self, message: &str) {
        if self.debug {
            eprintln!("fili-im: {message}");
        }
    }

    fn broadcast(&self, event: Value) {
        let _ = self.events.send(event);
    }

    fn start(self: &Arc<Self>, key: &str, platform: &str) {
        let conn = Connector::new(key, platform);
        let task = tokio::spawn(connector::supervise(self.clone(), conn.clone()));
        self.connectors.lock().unwrap().insert(key.into(), Running { conn, task });
    }

    fn connector(&self, key: &str) -> Option<Arc<Connector>> {
        self.connectors.lock().unwrap().get(key).map(|r| r.conn.clone())
    }

    /// Stop an account's connector and wait for its process to end.
    async fn stop(&self, key: &str) {
        let running = self.connectors.lock().unwrap().remove(key);
        if let Some(r) = running {
            r.conn.stop();
            let _ = tokio::time::timeout(Duration::from_secs(5), r.task).await;
        }
    }

    fn account_changed(&self, key: &str) {
        if let Ok(Some(account)) = self.store.lock().unwrap().account(key) {
            self.broadcast(json!({"event": "account", "account": account}));
        }
    }

    fn chat_changed(&self, chat: i64) {
        if let Ok(Some(chat)) = self.store.lock().unwrap().chat(chat) {
            self.broadcast(json!({"event": "chat", "chat": chat}));
        }
    }

    fn message_changed(&self, id: i64) {
        if let Ok(Some(message)) = self.store.lock().unwrap().message_view(id) {
            self.broadcast(json!({"event": "message", "message": message}));
        }
    }

    fn refresh_alerts(&self) {
        let (total, recent) = {
            let store = self.store.lock().unwrap();
            (store.unread_total().unwrap_or(0), store.recent_unread(notify::RECENT).unwrap_or_default())
        };
        self.alerts.write(total, &recent);
    }

    /// The connector process is not usable (missing, crashed): say so on the account.
    fn connector_down(&self, conn: &Connector, state: &str, detail: &str) {
        eprintln!("fili-im: {}: {detail}", conn.key);
        let _ = self.store.lock().unwrap().set_state(&conn.key, state, Some(detail), None);
        self.account_changed(&conn.key);
    }

    /// Handle one event from a connector. Returns false when the connector must be stopped (incompatible protocol).
    fn on_event(self: &Arc<Self>, conn: &Arc<Connector>, event: Event) -> bool {
        let key = conn.key.as_str();
        let store = || self.store.lock().unwrap();
        let logged = |what: &str, r: anyhow::Result<()>| {
            if let Err(e) = r {
                eprintln!("fili-im: {key}: {what}: {e:#}");
            }
        };
        match event {
            Event::Hello { protocol, platform, name, capabilities, .. } => {
                if protocol != crate::protocol::PROTOCOL || platform != conn.platform {
                    let detail = format!(
                        "Incompatible connector (protocol {protocol}, platform {platform}); this fili-im speaks protocol {} for {}",
                        crate::protocol::PROTOCOL,
                        conn.platform
                    );
                    self.connector_down(conn, "error", &detail);
                    return false;
                }
                let name = if name.is_empty() { platform.clone() } else { name };
                logged("hello", store().set_hello(key, &name, &capabilities));
                conn.set_capabilities(Some(capabilities));
                self.account_changed(key);
            }
            Event::State { state, detail, me, .. } => {
                logged("state", store().set_state(key, state.as_str(), detail.as_deref(), me.as_ref()));
                conn.set_connected(state == ConnState::Connected);
                self.account_changed(key);
                if state == ConnState::Connected {
                    tokio::spawn(flush(self.clone(), conn.clone()));
                }
            }
            Event::Pairing { kind, data, expires_s } => {
                self.broadcast(json!({"event": "pairing", "account": key, "kind": kind, "data": data, "expires_s": expires_s}));
            }
            Event::ChatUpsert { chat } => {
                let upserted = store().upsert_chat(key, &chat);
                match upserted {
                    Ok(id) => {
                        self.chat_changed(id);
                        self.refresh_alerts();
                    }
                    Err(e) => eprintln!("fili-im: {key}: chat: {e:#}"),
                }
            }
            Event::ContactUpsert { contact } => {
                logged("contact", store().upsert_contact(key, &contact));
                self.broadcast(json!({"event": "contacts", "account": key}));
            }
            Event::MessageNew { live, message } => {
                let stored = store().put_message(key, &message, live);
                match stored {
                    Ok(stored) => {
                        self.message_changed(stored.id);
                        self.chat_changed(stored.chat);
                        if live && stored.new && !message.from_me {
                            self.announce(stored.chat, &message);
                        }
                        self.refresh_alerts();
                    }
                    Err(e) => eprintln!("fili-im: {key}: message: {e:#}"),
                }
            }
            Event::HistoryBatch { messages, progress } => {
                let stored = store().put_history(key, &messages);
                match stored {
                    Ok(stored) => {
                        let mut chats: Vec<i64> = stored.iter().map(|s| s.chat).collect();
                        chats.sort_unstable();
                        chats.dedup();
                        for chat in chats {
                            self.chat_changed(chat);
                        }
                        self.broadcast(json!({"event": "history", "account": key, "count": stored.len(), "progress": progress}));
                    }
                    Err(e) => eprintln!("fili-im: {key}: history: {e:#}"),
                }
            }
            Event::MessageEdit { chat, id, text, timestamp } => {
                let edited = store().edit_message(key, &chat, &id, &text, timestamp);
                if let Ok(Some(id)) = edited {
                    self.message_changed(id);
                    self.refresh_alerts();
                }
            }
            Event::MessageDelete { chat, id } => {
                let deleted = store().delete_message(key, &chat, &id);
                if let Ok(Some(id)) = deleted {
                    self.message_changed(id);
                    self.refresh_alerts();
                }
            }
            Event::Reaction { chat, message, sender, from_me, emoji, timestamp } => {
                let sender = if from_me { None } else { sender.as_deref() };
                let reacted = store().react(key, &chat, &message, sender, &emoji, timestamp);
                if let Ok(Some(id)) = reacted {
                    self.message_changed(id);
                }
            }
            Event::Receipt { chat, messages, kind, from_me, .. } => {
                let changed = store().receipt(key, &chat, &messages, kind, from_me).unwrap_or_default();
                for id in changed {
                    self.message_changed(id);
                }
                if from_me {
                    let found = store().find_chat(key, &chat);
                    if let Ok(Some(chat)) = found {
                        self.chat_changed(chat);
                    }
                    self.refresh_alerts();
                }
            }
            Event::Typing { chat, sender, active } => {
                let found = store().find_chat(key, &chat);
                if let Ok(Some(chat)) = found {
                    self.broadcast(json!({"event": "typing", "account": key, "chat": chat, "sender": sender, "active": active}));
                }
            }
            Event::Error { message } => {
                eprintln!("fili-im: {key}: {message}");
                self.broadcast(json!({"event": "account.error", "account": key, "message": message}));
            }
        }
        true
    }

    /// A desktop notification for a new incoming message, unless its chat is muted.
    fn announce(self: &Arc<Self>, chat: i64, message: &crate::protocol::Message) {
        let Ok(Some(view)) = self.store.lock().unwrap().chat(chat) else { return };
        if view["muted"].as_bool().unwrap_or(false) {
            return;
        }
        let title = view["title"].as_str().unwrap_or("").to_string();
        let media = message.media.as_ref().and_then(|m| serde_json::to_string(m).ok());
        let body = message.text.as_ref().map(|t| t.plain.as_str()).unwrap_or("");
        let line = preview(&message.kind, body, media.as_deref());
        let line = match (view["kind"].as_str(), view["last"]["sender_name"].as_str().or(message.sender.as_deref())) {
            (Some("direct"), _) | (_, None) => line,
            (_, Some(sender)) => format!("{sender}: {line}"),
        };
        let daemon = self.clone();
        self.notifier.announce(chat, &title, &line, move |action| daemon.on_notification(chat, action));
    }

    /// The user answered a notification: show the conversation, mark it read, or mute it for an hour.
    fn on_notification(self: &Arc<Self>, chat: i64, action: notify::Action) {
        use notify::Action;
        match action {
            Action::Open | Action::Reply => {
                let mut actions = vec![format!("open-chat:{chat}")];
                if action == Action::Reply {
                    actions.push("focus-composer".into());
                }
                crate::ctl::show_window(&actions);
            }
            Action::MarkRead => {
                let _ = control::mark_read(self, chat, crate::policy::Ask::Explicit);
            }
            Action::Mute => {
                let until = crate::store::now_ms() + 3_600_000;
                let done = self.store.lock().unwrap().mute(chat, until);
                if done.is_ok() {
                    self.chat_changed(chat);
                    self.refresh_alerts();
                }
            }
        }
    }

    // -----------------------------------------------------------------------------------------------------------
    // Actions behind the control socket

    /// Queue a message and hand the outbox to the connector.
    fn send(
        self: &Arc<Self>,
        chat: i64,
        text: Text,
        reply_to: Option<String>,
        media: Option<crate::protocol::Media>,
    ) -> Result<Value, crate::protocol::ErrorBody> {
        use crate::protocol::ErrorBody;
        let store = self.store.lock().unwrap();
        let Some((account, _, _)) = store.chat_ref(chat).map_err(internal)? else {
            return Err(ErrorBody::new("not_found", "no such conversation"));
        };
        if text.plain.trim().is_empty() && media.is_none() {
            return Err(ErrorBody::new("invalid", "the message is empty"));
        }
        let mut params = Map::new();
        if let Some(reply_to) = &reply_to {
            params.insert("reply_to".into(), json!(reply_to));
        }
        if let Some(media) = &media {
            params.insert("media".into(), json!({"kind": media.kind}));
        }
        let caps = store.capabilities(&account).map_err(internal)?;
        caps.allows("send", &params).map_err(|m| ErrorBody::new("unsupported", m))?;
        if let (Some(media), Some(max)) = (&media, caps.media_max_bytes)
            && media.size.is_some_and(|s| s > max)
        {
            return Err(ErrorBody::new("too_large", format!("the file is larger than {} MB", max / 1_000_000)));
        }
        let id = store.queue(chat, &text, reply_to.as_deref(), media.as_ref(), &client_id()).map_err(internal)?;
        let view = store.message_view(id).map_err(internal)?.unwrap_or(Value::Null);
        drop(store);
        self.broadcast(json!({"event": "message", "message": view}));
        self.chat_changed(chat);
        if let Some(conn) = self.connector(&account) {
            tokio::spawn(flush(self.clone(), conn));
        }
        Ok(view)
    }
}

fn internal(e: anyhow::Error) -> crate::protocol::ErrorBody {
    crate::protocol::ErrorBody::new("failed", format!("{e:#}"))
}

/// A unique id for an outgoing message (idempotent sends after a restart).
fn client_id() -> String {
    let mut bytes = [0u8; 12];
    // SAFETY: getrandom writes at most len bytes into the buffer.
    let n = unsafe { libc::getrandom(bytes.as_mut_ptr().cast(), bytes.len(), 0) };
    if n != bytes.len() as isize {
        let t = now_ms() as u128 ^ (std::process::id() as u128) << 64;
        bytes.copy_from_slice(&t.to_le_bytes()[..12]);
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Send what is waiting for this account, oldest first; stops at the first sign the connector cannot send now.
async fn flush(daemon: Arc<Daemon>, conn: Arc<Connector>) {
    let _one = conn.flush.lock().await;
    if !conn.is_connected() {
        return;
    }
    let pending = daemon.store.lock().unwrap().pending(&conn.key).unwrap_or_default();
    for out in pending {
        if !dispatch(&daemon, &conn, out).await {
            break;
        }
    }
}

/// One message to the connector; false when the rest of the outbox should wait.
async fn dispatch(daemon: &Arc<Daemon>, conn: &Arc<Connector>, out: Outgoing) -> bool {
    let mut params = Map::new();
    params.insert("chat".into(), json!(out.chat));
    params.insert("client_id".into(), json!(out.client_id));
    params.insert("text".into(), serde_json::to_value(&out.text).unwrap_or(Value::Null));
    if let Some(media) = &out.media {
        params.insert("media".into(), serde_json::to_value(media).unwrap_or(Value::Null));
    }
    if let Some(reply_to) = &out.reply_to {
        params.insert("reply_to".into(), json!(reply_to));
    }
    match conn.command("send", params, SEND_TIMEOUT).await {
        Ok(result) => {
            let native = result.get("id").and_then(Value::as_str).unwrap_or_default().to_string();
            if native.is_empty() {
                let _ = daemon.store.lock().unwrap().failed(&out.client_id, "the connector gave no message id");
                daemon.message_changed(out.id);
                return true;
            }
            let ts = result.get("timestamp").and_then(Value::as_i64).unwrap_or_else(now_ms);
            let kept = daemon.store.lock().unwrap().sent(&out.client_id, &native, ts);
            if let Ok(Some(id)) = kept {
                daemon.message_changed(id);
                let view = daemon.store.lock().unwrap().message_view(id);
                if let Ok(Some(view)) = view
                    && let Some(chat) = view["chat"].as_i64()
                {
                    daemon.chat_changed(chat);
                }
            }
            true
        }
        Err(e) if e.code == "not_connected" || e.code == "rate_limited" => false,
        Err(e) => {
            let _ = daemon.store.lock().unwrap().failed(&out.client_id, &e.message);
            daemon.message_changed(out.id);
            true
        }
    }
}
