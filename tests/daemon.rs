//! The daemon end to end with the fake connector, in a private HOME and runtime directory: add and pair an account,
//! receive, send, read, search, restart, remove.
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct World {
    root: PathBuf,
}

impl World {
    fn new() -> World {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        // Short: a Unix socket path is limited to 108 bytes.
        let root = std::env::temp_dir().join(format!("fim-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("home")).unwrap();
        std::fs::create_dir_all(root.join("run")).unwrap();
        World { root }
    }
    fn home(&self) -> PathBuf {
        self.root.join("home")
    }
    fn socket(&self) -> PathBuf {
        self.root.join("run/fili-im/daemon.sock")
    }
    fn daemon(&self) -> Daemon {
        self.daemon_with(&[])
    }
    fn daemon_with(&self, extra: &[(&str, &str)]) -> Daemon {
        let fake = Path::new(env!("CARGO_BIN_EXE_fili-im-fake"));
        let child = Command::new(env!("CARGO_BIN_EXE_fili-im"))
            .arg("daemon")
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", self.home())
            .env("XDG_RUNTIME_DIR", self.root.join("run"))
            .env("FILI_IM_CONNECTOR_DIR", fake.parent().unwrap())
            .env("FILI_IM_NO_NOTIFY", "1")
            .env("FILI_IM_FAKE_PAIR_MS", "50")
            .env("FILI_IM_FAKE_REPLY_MS", "50")
            .envs(extra.iter().copied())
            .stdin(Stdio::null())
            .stderr(if std::env::var_os("FILI_IM_TEST_STDERR").is_some() { Stdio::inherit() } else { Stdio::null() })
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !self.socket().exists() {
            assert!(Instant::now() < deadline, "the daemon did not open its socket");
            std::thread::sleep(Duration::from_millis(20));
        }
        Daemon { child }
    }
    fn client(&self) -> Client {
        let stream = UnixStream::connect(self.socket()).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        Client { reader: BufReader::new(stream.try_clone().unwrap()), writer: stream, next: 1, events: Vec::new() }
    }
    fn alerts(&self, name: &str) -> String {
        std::fs::read_to_string(self.home().join(".local/state/alerts").join(name)).unwrap_or_default()
    }
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

struct Daemon {
    child: Child,
}

impl Daemon {
    fn stop(mut self, client: &mut Client) {
        client.call("shutdown", json!({}));
        let deadline = Instant::now() + Duration::from_secs(10);
        while self.child.try_wait().unwrap().is_none() {
            assert!(Instant::now() < deadline, "the daemon did not stop");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct Client {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
    next: u64,
    events: Vec<Value>,
}

impl Client {
    fn read(&mut self) -> Value {
        let mut line = String::new();
        self.reader.read_line(&mut line).expect("read from the daemon");
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("bad line {line:?}: {e}"))
    }

    fn try_call(&mut self, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        writeln!(self.writer, "{}", json!({"id": id, "method": method, "params": params})).unwrap();
        loop {
            let value = self.read();
            if value.get("reply").and_then(Value::as_u64) == Some(id) {
                return value;
            }
            if value.get("event").is_some() {
                self.events.push(value);
            }
        }
    }

    fn call(&mut self, method: &str, params: Value) -> Value {
        let reply = self.try_call(method, params);
        assert_eq!(reply["ok"], true, "{method} failed: {reply}");
        reply["result"].clone()
    }

    fn wait_event(&mut self, pred: impl Fn(&Value) -> bool) -> Value {
        if let Some(i) = self.events.iter().position(&pred) {
            return self.events.remove(i);
        }
        loop {
            let value = self.read();
            if pred(&value) {
                return value;
            }
        }
    }

    /// Poll until `pred` holds on a call's result.
    fn until(&mut self, method: &str, params: Value, pred: impl Fn(&Value) -> bool) -> Value {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let result = self.call(method, params.clone());
            if pred(&result) {
                return result;
            }
            assert!(Instant::now() < deadline, "{method} never satisfied the condition: {result}");
            std::thread::sleep(Duration::from_millis(30));
        }
    }
}

fn chat_id(chats: &Value, title: &str) -> i64 {
    chats.as_array().unwrap().iter().find(|c| c["title"] == title).unwrap_or_else(|| panic!("no chat {title}: {chats}"))["id"]
        .as_i64()
        .unwrap()
}

#[test]
fn daemon_end_to_end() {
    let world = World::new();
    let daemon = world.daemon();
    let mut c = world.client();

    assert_eq!(c.call("hello", json!({}))["api"], 1);
    let platforms = c.call("platforms.list", json!({}));
    assert!(platforms.as_array().unwrap().iter().any(|p| p["platform"] == "fake"), "{platforms}");
    let unknown = c.try_call("account.add", json!({"platform": "nope"}));
    assert_eq!(unknown["error"]["code"], "not_found");

    // Add and pair.
    c.call("subscribe", json!({}));
    let account = c.call("account.add", json!({"platform": "fake"}));
    assert_eq!(account["key"], "fake-1");
    c.wait_event(|e| e["event"] == "account" && e["account"]["state"] == "needs_pairing");
    let refused = c.try_call("account.pair", json!({"account": "fake-1", "method": "password"}));
    assert_eq!(refused["error"]["code"], "unsupported", "capabilities are a ceiling");
    c.call("account.pair", json!({"account": "fake-1"}));
    let pairing = c.wait_event(|e| e["event"] == "pairing");
    assert_eq!(pairing["data"], "FAKE-QR-1");
    c.wait_event(|e| e["event"] == "account" && e["account"]["state"] == "connected");
    let dir = world.home().join(".local/share/fili-im/accounts/fake-1");
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700, "account directory is private");

    // History arrived, not counted as unread, titles resolved.
    let chats = c.until("chats.list", json!({}), |r| r.as_array().is_some_and(|a| a.len() == 2));
    let paul = chat_id(&chats, "Paul");
    let family = chat_id(&chats, "Family");
    assert_eq!(c.call("status", json!({}))["unread"], 0);

    // Send: pending, then sent, then the echo arrives as unread and reaches the bar's files.
    let sent = c.call("send", json!({"chat": paul, "text": "On se voit à midi ?"}));
    assert_eq!(sent["state"], "pending");
    let id = sent["id"].as_i64().unwrap();
    c.wait_event(|e| e["event"] == "message" && e["message"]["id"] == id && e["message"]["state"] == "sent");
    c.wait_event(|e| e["event"] == "message" && e["message"]["text"]["plain"] == "Echo: On se voit à midi ?");
    c.until("status", json!({}), |s| s["unread"] == 1);
    assert_eq!(world.alerts("messages.count"), "1\n");
    assert_eq!(world.alerts("messages"), "Paul\tEcho: On se voit à midi ?\n");
    let empty = c.try_call("send", json!({"chat": paul, "text": "   "}));
    assert_eq!(empty["error"]["code"], "invalid");

    // Group: the sender prefixes the bar line.
    c.call("send", json!({"chat": family, "text": "Moi"}));
    c.until("status", json!({}), |s| s["unread"] == 2);
    assert!(world.alerts("messages").starts_with("Family\tLéa: Echo: Moi\n"), "{}", world.alerts("messages"));

    // Read: unread drops, nothing sent to the platform unless asked.
    let read = c.call("mark_read", json!({"chat": paul}));
    assert_eq!(read["receipt"], false);
    let read = c.call("mark_read", json!({"chat": family, "receipt": true}));
    assert_eq!(read["receipt"], true);
    assert_eq!(c.call("status", json!({}))["unread"], 0);
    assert_eq!(world.alerts("messages.count"), "0\n");
    assert_eq!(world.alerts("messages"), "");

    // React and edit go through the connector, then apply here.
    let messages = c.call("messages.list", json!({"chat": paul}));
    let mine = messages.as_array().unwrap().iter().find(|m| m["id"] == id).unwrap().clone();
    c.call("react", json!({"message": id, "emoji": "👍"}));
    c.call("edit", json!({"message": id, "text": "On se voit à 13 h ?"}));
    let after = c.call("messages.list", json!({"chat": paul}));
    let edited = after.as_array().unwrap().iter().find(|m| m["id"] == id).unwrap();
    assert_eq!(edited["edited"], true);
    assert_eq!(edited["reactions"][0]["emoji"], "👍");
    assert_ne!(mine["text"], edited["text"]);

    // Search.
    let hits = c.call("search", json!({"query": "dessert"}));
    assert_eq!(hits.as_array().unwrap().len(), 1);
    assert_eq!(hits[0]["chat_title"], "Family");

    // Presence, shared files and profile pictures.
    assert_eq!(c.call("presence.get", json!({}))["state"], "away");
    assert_eq!(c.try_call("presence.set", json!({"state": "dnd"}))["error"]["code"], "invalid");
    c.call("presence.set", json!({"state": "busy"}));
    assert_eq!(c.call("presence.get", json!({}))["state"], "busy");
    assert_eq!(c.try_call("avatar.fetch", json!({"chat": paul}))["error"]["code"], "not_found");
    assert_eq!(c.call("chat.media", json!({"chat": paul})), json!([]));
    c.call("presence.set", json!({"state": "offline"}));
    c.until("accounts.list", json!({}), |a| a[0]["state"] == "offline");
    c.call("presence.set", json!({"state": "away"}));
    c.until("accounts.list", json!({}), |a| a[0]["state"] == "connected");

    // Restart: data stays, the account reconnects without pairing.
    daemon.stop(&mut c);
    let daemon = world.daemon();
    let mut c = world.client();
    c.until("accounts.list", json!({}), |a| a[0]["state"] == "connected");
    let chats = c.call("chats.list", json!({}));
    assert_eq!(chats.as_array().unwrap().len(), 2);

    // Remove: logged out, data gone.
    c.call("account.remove", json!({"account": "fake-1"}));
    assert!(!dir.exists(), "the account directory is removed");
    assert_eq!(c.call("accounts.list", json!({})), json!([]));
    assert_eq!(c.call("chats.list", json!({})), json!([]));
    daemon.stop(&mut c);
}

#[test]
fn a_second_daemon_exits_quietly() {
    // Own world: the first test may run in parallel.
    let root = std::env::temp_dir().join(format!("fili-im-daemon-single-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("run")).unwrap();
    let start = || {
        Command::new(env!("CARGO_BIN_EXE_fili-im"))
            .arg("daemon")
            .env("HOME", &root)
            .env("XDG_RUNTIME_DIR", root.join("run"))
            .env_remove("XDG_DATA_HOME")
            .env_remove("XDG_STATE_HOME")
            .stderr(Stdio::null())
            .spawn()
            .unwrap()
    };
    let mut first = start();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !root.join("run/fili-im/daemon.sock").exists() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(20));
    }
    let status = start().wait().unwrap();
    assert!(status.success());
    assert!(first.try_wait().unwrap().is_none(), "the first daemon keeps running");
    let _ = first.kill();
    let _ = first.wait();
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_crashed_connector_restarts_and_the_outbox_waits() {
    let world = World::new();
    let daemon = world.daemon_with(&[("FILI_IM_FAKE_AUTOPAIR", "1"), ("FILI_IM_FAKE_CRASH_MS", "400")]);
    let mut c = world.client();
    c.call("subscribe", json!({}));
    c.call("account.add", json!({"platform": "fake"}));
    let chats = c.until("chats.list", json!({}), |r| r.as_array().is_some_and(|a| a.len() == 2));
    let paul = chat_id(&chats, "Paul");
    // The crash: the account says so, and a message sent meanwhile stays in the outbox.
    c.wait_event(|e| e["event"] == "account" && e["account"]["state"] == "error");
    let sent = c.call("send", json!({"chat": paul, "text": "Pendant la panne"}));
    let id = sent["id"].as_i64().unwrap();
    // Restarted after one second of back-off; the outbox goes out on `connected`.
    c.wait_event(|e| e["event"] == "account" && e["account"]["state"] == "connected");
    c.wait_event(|e| e["event"] == "message" && e["message"]["id"] == id && e["message"]["state"] == "sent");
    let log = std::fs::read_to_string(world.home().join(".local/state/fili-im/fake-1.log")).unwrap_or_default();
    assert!(!log.contains("Pendant la panne"), "message contents never reach the log");
    daemon.stop(&mut c);
}
