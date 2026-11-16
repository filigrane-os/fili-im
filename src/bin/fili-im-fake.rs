//! `fili-im-fake`: a connector that talks to nobody. It follows the connector protocol so the daemon, the window and the
//! conformance suite can be worked on without a real account.
//!
//! - First start: `needs_pairing`; `pair` shows a QR (or a code), then connects after `FILI_IM_FAKE_PAIR_MS` (300).
//!   `FILI_IM_FAKE_AUTOPAIR=1` skips pairing.
//! - Connected: Paul (direct) and Family (group), a little history; every message sent gets an "Echo: …" answer from
//!   the chat after `FILI_IM_FAKE_REPLY_MS` (500).
//! - `FILI_IM_FAKE_INCOMING_S=N`: a new message from Paul every N seconds (interface work).
//! - `FILI_IM_FAKE_CRASH_MS=N`: the first run exits with status 3 after N ms (tests the daemon's restart).
use fili_im::protocol::*;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

struct Out(Mutex<std::io::Stdout>);

impl Out {
    fn line(&self, value: &impl serde::Serialize) {
        let mut out = self.0.lock().unwrap();
        let _ = serde_json::to_writer(&mut *out, value);
        let _ = out.write_all(b"\n");
        let _ = out.flush();
    }
    fn event(&self, event: Event) {
        self.line(&event);
    }
}

fn env_ms(name: &str, default: u64) -> Duration {
    Duration::from_millis(std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default))
}

fn now() -> i64 {
    fili_im::store::now_ms()
}

fn capabilities() -> Capabilities {
    Capabilities {
        pairing: vec!["qr".into(), "code".into()],
        message_kinds: vec!["text".into(), "voice".into()],
        edit: Some(Limit { window_s: Some(900) }),
        delete: Some(Limit::default()),
        reactions: Reactions::OnePerUser,
        replies: true,
        threads: false,
        receipts: Receipts { send: true, suppress: true },
        typing: Typing { send: true, receive: true },
        history: History::OnDemand,
        schedule_native: false,
        media_max_bytes: None,
        chat_kinds: vec!["direct".into(), "group".into()],
        formatting: Formatting::Spans,
        identity: Identity { phone: true },
        presence: true,
        avatars: true,
        streaming: false,
    }
}

fn message(id: &str, chat: &str, sender: Option<&str>, ts: i64, text: &str) -> Message {
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

struct Fake {
    out: Arc<Out>,
    session: PathBuf,
    /// client_id → message id: `send` is idempotent.
    sent: Mutex<HashMap<String, String>>,
    counter: Mutex<u64>,
    connected: Mutex<bool>,
}

impl Fake {
    fn next_id(&self) -> String {
        let mut c = self.counter.lock().unwrap();
        *c += 1;
        format!("fake-{}-{}", std::process::id(), *c)
    }

    fn connect(&self) {
        *self.connected.lock().unwrap() = true;
        self.out.event(Event::State {
            state: ConnState::Connected,
            detail: None,
            me: Some(SelfInfo { id: "me".into(), name: Some("Me".into()), phone: Some("+33600000000".into()) }),
            logout_in_s: None,
        });
        if std::env::var("FILI_IM_FAKE_SCENE").as_deref() == Ok("sam") {
            self.sam_scene();
            return;
        }
        for (id, name, phone) in [("paul", "Paul", "+33611111111"), ("lea", "Léa", "+33622222222")] {
            self.out.event(Event::ContactUpsert {
                contact: Contact { id: id.into(), name: Some(name.into()), phone: Some(phone.into()), ..Default::default() },
            });
        }
        self.out.event(Event::ChatUpsert { chat: Chat { id: "paul".into(), kind: Some("direct".into()), ..Default::default() } });
        self.out.event(Event::ChatUpsert {
            chat: Chat { id: "family".into(), kind: Some("group".into()), title: Some("Family".into()), ..Default::default() },
        });
        let t = now() - 3_600_000;
        self.out.event(Event::HistoryBatch {
            messages: vec![
                message("h1", "paul", Some("paul"), t, "Salut, tu es dispo demain ?"),
                message("h2", "paul", None, t + 60_000, "Oui, vers midi"),
                message("h3", "family", Some("lea"), t + 120_000, "Qui ramène le dessert dimanche ?"),
            ],
            progress: Some(Progress { done: 3, total: Some(3) }),
        });
    }

    /// A one-conversation scene for looking at how a second platform shows up next to WhatsApp (development only):
    /// a contact without a phone number (a username, as on Telegram) and a short conversation.
    fn sam_scene(&self) {
        self.out.event(Event::ContactUpsert {
            contact: Contact { id: "sam".into(), name: Some("Sam".into()), ..Default::default() },
        });
        self.out.event(Event::ChatUpsert { chat: Chat { id: "sam".into(), kind: Some("direct".into()), ..Default::default() } });
        let t = now() - 2 * 3_600_000;
        let lines: [(bool, &str); 6] = [
            (false, "Coucou, tu rentres à quelle heure ce soir ?"),
            (true, "Vers 19 h, je prends le pain en passant"),
            (false, "Parfait ! On regarde un film ?"),
            (true, "Avec plaisir, tu choisis"),
            (false, "https://www.youtube.com/watch?v=dQw4w9WgXcQ"),
            (false, "Celui-là, haha"),
        ];
        let messages = lines
            .iter()
            .enumerate()
            .map(|(i, (mine, text))| {
                message(&format!("d{i}"), "sam", if *mine { None } else { Some("sam") }, t + i as i64 * 90_000, text)
            })
            .collect();
        self.out.event(Event::HistoryBatch { messages, progress: Some(Progress { done: 6, total: Some(6) }) });
    }

    fn reply_later(self: &Arc<Self>, chat: String, text: String) {
        let fake = self.clone();
        std::thread::spawn(move || {
            std::thread::sleep(env_ms("FILI_IM_FAKE_REPLY_MS", 500));
            let sender = if chat == "family" {
                "lea"
            } else if chat == "sam" {
                "sam"
            } else {
                "paul"
            };
            let id = fake.next_id();
            fake.out.event(Event::MessageNew {
                live: true,
                message: message(&id, &chat, Some(sender), now(), &format!("Echo: {text}")),
            });
        });
    }

    fn handle(self: &Arc<Self>, cmd: &Command) -> Result<Value, (&'static str, String)> {
        let p = &cmd.params;
        let s = |name: &str| p.get(name).and_then(Value::as_str).unwrap_or("").to_string();
        let need_connected =
            || if *self.connected.lock().unwrap() { Ok(()) } else { Err(("not_connected", "not paired".to_string())) };
        let known_chat = |chat: &str| {
            if chat == "paul" || chat == "family" || chat == "sam" {
                Ok(())
            } else {
                Err(("not_found", format!("no chat {chat:?}")))
            }
        };
        match cmd.cmd.as_str() {
            "pair" => {
                let method = s("method");
                if method != "qr" && method != "code" {
                    return Err(("unsupported", format!("pairing by {method:?}")));
                }
                let (kind, data) = if method == "code" { ("code", "FAKE-CODE") } else { ("qr", "FAKE-QR-1") };
                let fake = self.clone();
                std::thread::spawn(move || {
                    fake.out.event(Event::Pairing { kind: kind.into(), data: data.into(), expires_s: Some(20) });
                    std::thread::sleep(env_ms("FILI_IM_FAKE_PAIR_MS", 300));
                    let _ = std::fs::write(&fake.session, "paired\n");
                    fake.connect();
                });
                Ok(json!({}))
            }
            "logout" => {
                let _ = std::fs::remove_file(&self.session);
                *self.connected.lock().unwrap() = false;
                self.out.event(Event::State { state: ConnState::LoggedOut, detail: None, me: None, logout_in_s: None });
                Ok(json!({}))
            }
            "send" => {
                need_connected()?;
                let chat = s("chat");
                known_chat(&chat)?;
                let client = s("client_id");
                let text: Text = p.get("text").cloned().and_then(|t| serde_json::from_value(t).ok()).unwrap_or_default();
                if text.plain.is_empty() && p.get("media").is_none_or(Value::is_null) {
                    return Err(("invalid", "empty message".into()));
                }
                let mut sent = self.sent.lock().unwrap();
                if let Some(id) = sent.get(&client) {
                    return Ok(json!({"id": id, "timestamp": now()}));
                }
                let id = self.next_id();
                if !client.is_empty() {
                    sent.insert(client, id.clone());
                }
                drop(sent);
                let echo = if text.plain.is_empty() { "[file]".to_string() } else { text.plain };
                self.reply_later(chat, echo);
                Ok(json!({"id": id, "timestamp": now()}))
            }
            "presence" => {
                need_connected()?;
                Ok(json!({}))
            }
            "avatar" => Err(("not_found", "no profile picture".into())),
            "edit" | "delete" | "react" | "mark_read" | "typing" | "chat.mute" | "chat.archive" => {
                need_connected()?;
                known_chat(&s("chat"))?;
                Ok(json!({}))
            }
            "fetch_media" => {
                let dest = s("dest");
                if dest.is_empty() {
                    return Err(("invalid", "missing dest".into()));
                }
                std::fs::write(&dest, b"fake media\n").map_err(|e| ("failed", e.to_string()))?;
                Ok(json!({"path": dest}))
            }
            "backfill" => {
                need_connected()?;
                let chat = s("chat");
                known_chat(&chat)?;
                let t = now() - 86_400_000;
                let sender = if chat == "family" { "lea" } else { "paul" };
                self.out.event(Event::HistoryBatch {
                    messages: vec![
                        message(&format!("old-{chat}-1"), &chat, Some(sender), t, "Un vieux message"),
                        message(&format!("old-{chat}-2"), &chat, None, t + 1000, "Une vieille réponse"),
                    ],
                    progress: None,
                });
                Ok(json!({}))
            }
            "shutdown" => Ok(json!({})),
            other => Err(("unsupported", format!("unknown command {other:?}"))),
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let arg = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let data = PathBuf::from(arg("--data").unwrap_or_else(|| ".".into()));
    let fake = Arc::new(Fake {
        out: Arc::new(Out(Mutex::new(std::io::stdout()))),
        session: data.join("session"),
        sent: Mutex::new(HashMap::new()),
        counter: Mutex::new(0),
        connected: Mutex::new(false),
    });
    let platform = std::env::var("FILI_IM_FAKE_PLATFORM").unwrap_or_else(|_| "fake".into());
    fake.out.event(Event::Hello {
        protocol: PROTOCOL,
        platform: platform.clone(),
        name: if platform == "fake" { "Fake".into() } else { "Telegram (simulation)".into() },
        version: env!("CARGO_PKG_VERSION").into(),
        capabilities: capabilities(),
    });
    if fake.session.exists() || std::env::var_os("FILI_IM_FAKE_AUTOPAIR").is_some() {
        fake.out.event(Event::State { state: ConnState::Connecting, detail: None, me: None, logout_in_s: None });
        fake.connect();
    } else {
        fake.out.event(Event::State { state: ConnState::NeedsPairing, detail: None, me: None, logout_in_s: None });
    }
    if let Some(ms) = std::env::var("FILI_IM_FAKE_CRASH_MS").ok().and_then(|v| v.parse::<u64>().ok()) {
        let marker = data.join("crashed");
        if !marker.exists() {
            let _ = std::fs::write(&marker, "");
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(ms));
                std::process::exit(3);
            });
        }
    }
    if let Some(every) = std::env::var("FILI_IM_FAKE_INCOMING_S").ok().and_then(|v| v.parse::<u64>().ok()).filter(|n| *n > 0) {
        let fake = fake.clone();
        std::thread::spawn(move || {
            let mut n = 0;
            loop {
                std::thread::sleep(Duration::from_secs(every));
                if *fake.connected.lock().unwrap() {
                    n += 1;
                    let id = fake.next_id();
                    fake.out.event(Event::MessageNew {
                        live: true,
                        message: message(&id, "paul", Some("paul"), now(), &format!("Message {n}")),
                    });
                }
            }
        });
    }
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let value: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("fili-im-fake: bad line: {e}");
                continue;
            }
        };
        let id = value.get("id").and_then(Value::as_u64).unwrap_or(0);
        let command: Command = match serde_json::from_value(value) {
            Ok(c) => c,
            Err(e) => {
                fake.out.line(&Reply::err(id, "invalid", e.to_string()));
                continue;
            }
        };
        let reply = match fake.handle(&command) {
            Ok(result) => Reply::ok(command.id, result),
            Err((code, message)) => Reply::err(command.id, code, message),
        };
        fake.out.line(&reply);
        if command.cmd == "shutdown" {
            break;
        }
    }
}
