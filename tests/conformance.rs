//! Conformance suite for connectors (docs/connector-protocol.md). Runs against `fili-im-fake` by default; against a
//! real connector with `FILI_IM_CONNECTOR=/path/to/fili-im-<platform>`. Tests that need a connected account use
//! `FILI_IM_CONFORMANCE_DATA` (an already paired account directory) and `FILI_IM_CONFORMANCE_CHAT` (a chat id to
//! send to); with the fake connector they pair automatically.
use fili_im::protocol::{Capabilities, Event, Incoming, PROTOCOL, parse_line};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const WAIT: Duration = Duration::from_secs(10);

fn program() -> String {
    std::env::var("FILI_IM_CONNECTOR").unwrap_or_else(|_| env!("CARGO_BIN_EXE_fili-im-fake").to_string())
}

fn is_fake() -> bool {
    std::env::var_os("FILI_IM_CONNECTOR").is_none()
}

struct Session {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: mpsc::Receiver<String>,
    seen: Vec<Value>,
    next: u64,
    _dir: Option<TempDir>,
}

struct TempDir(std::path::PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp_dir(tag: &str) -> TempDir {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let dir =
        std::env::temp_dir().join(format!("fili-im-conf-{}-{tag}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
    std::fs::create_dir_all(&dir).unwrap();
    TempDir(dir)
}

impl Session {
    /// `connected`: a paired account (the fake pairs by itself; a real connector needs FILI_IM_CONFORMANCE_DATA).
    fn start(connected: bool) -> Option<Session> {
        let (data, dir) = match std::env::var("FILI_IM_CONFORMANCE_DATA") {
            Ok(data) if connected => (std::path::PathBuf::from(data), None),
            _ if connected && !is_fake() => return None,
            _ => {
                let dir = temp_dir("data");
                (dir.0.clone(), Some(dir))
            }
        };
        let mut command = Command::new(program());
        command
            .args(["--account", "test-1", "--data"])
            .arg(&data)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if connected && is_fake() {
            command.env("FILI_IM_FAKE_AUTOPAIR", "1").env("FILI_IM_FAKE_REPLY_MS", "50");
        }
        let mut child = command.spawn().expect("start the connector");
        let stdout = child.stdout.take().unwrap();
        let (tx, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let stdin = child.stdin.take();
        Some(Session { child, stdin, lines, seen: Vec::new(), next: 1, _dir: dir })
    }

    /// Next line, which must be a JSON object.
    fn next(&mut self, wait: Duration) -> Option<Value> {
        let line = self.lines.recv_timeout(wait).ok()?;
        let value: Value = serde_json::from_str(&line).unwrap_or_else(|e| panic!("not JSON ({e}): {line}"));
        assert!(value.is_object(), "not an object: {line}");
        if let Incoming::Invalid(why) = parse_line(&line) {
            panic!("invalid protocol line ({why}): {line}");
        }
        self.seen.push(value.clone());
        Some(value)
    }

    fn send_raw(&mut self, line: &str) {
        let stdin = self.stdin.as_mut().unwrap();
        stdin.write_all(line.as_bytes()).unwrap();
        stdin.write_all(b"\n").unwrap();
        stdin.flush().unwrap();
    }

    /// Send a command and wait for its reply; events in between are kept in `seen`.
    fn command(&mut self, cmd: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        let mut line = json!({"id": id, "cmd": cmd});
        for (k, v) in params.as_object().cloned().unwrap_or_default() {
            line[k] = v;
        }
        self.send_raw(&line.to_string());
        let deadline = Instant::now() + WAIT;
        while Instant::now() < deadline {
            if let Some(value) = self.next(deadline - Instant::now())
                && value.get("reply").and_then(Value::as_u64) == Some(id)
            {
                return value;
            }
        }
        panic!("no reply to {cmd}");
    }

    fn wait_event(&mut self, pred: impl Fn(&Value) -> bool) -> Option<Value> {
        if let Some(found) = self.seen.iter().find(|v| pred(v)) {
            return Some(found.clone());
        }
        let deadline = Instant::now() + WAIT;
        while Instant::now() < deadline {
            let value = self.next(deadline - Instant::now())?;
            if pred(&value) {
                return Some(value);
            }
        }
        None
    }

    fn exits_within(&mut self, wait: Duration) -> bool {
        let deadline = Instant::now() + wait;
        while Instant::now() < deadline {
            if self.child.try_wait().unwrap().is_some() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn hello(session: &mut Session) -> Capabilities {
    let first = session.next(WAIT).expect("the connector says nothing");
    let Incoming::Event(event) = parse_line(&first.to_string()) else { panic!("first line is not an event: {first}") };
    let Event::Hello { protocol, platform, capabilities, .. } = *event else { panic!("first line is not hello: {first}") };
    assert_eq!(protocol, PROTOCOL);
    assert!(fili_im::paths::valid_platform(&platform), "bad platform name {platform:?}");
    capabilities
}

#[test]
fn hello_comes_first_and_is_valid() {
    let mut s = Session::start(false).unwrap();
    let caps = hello(&mut s);
    assert!(!caps.message_kinds.is_empty());
    assert!(caps.message_kinds.iter().any(|k| k == "text"), "every connector sends text");
}

#[test]
fn every_command_gets_exactly_its_reply() {
    let mut s = Session::start(false).unwrap();
    hello(&mut s);
    let reply = s.command("no_such_command", json!({}));
    assert_eq!(reply["ok"], false);
    assert_eq!(reply["error"]["code"], "unsupported");
    // Unknown keys are ignored.
    let reply = s.command("fetch_media", json!({"future_key": 1}));
    assert!(reply.get("ok").is_some());
    let replies = s.seen.iter().filter(|v| v.get("reply").is_some()).count();
    assert_eq!(replies, 2, "one reply per command");
}

#[test]
fn shutdown_and_end_of_input_stop_the_connector() {
    let mut s = Session::start(false).unwrap();
    hello(&mut s);
    let reply = s.command("shutdown", json!({}));
    assert_eq!(reply["ok"], true);
    assert!(s.exits_within(Duration::from_secs(5)), "still running after shutdown");

    let mut s = Session::start(false).unwrap();
    hello(&mut s);
    drop(s.stdin.take());
    assert!(s.exits_within(Duration::from_secs(5)), "still running after end of input");
}

#[test]
fn pairing_methods_are_the_declared_ones() {
    let mut s = Session::start(false).unwrap();
    let caps = hello(&mut s);
    if caps.pairing.is_empty() {
        return;
    }
    let reply = s.command("pair", json!({"method": "carrier_pigeon"}));
    assert_eq!(reply["ok"], false, "an undeclared pairing method is refused");
    if is_fake() {
        let reply = s.command("pair", json!({"method": caps.pairing[0]}));
        assert_eq!(reply["ok"], true);
        assert!(s.wait_event(|v| v["event"] == "pairing").is_some(), "no pairing event");
        assert!(s.wait_event(|v| v["event"] == "state" && v["state"] == "connected").is_some(), "never connected");
    }
}

#[test]
fn send_is_idempotent_on_client_id() {
    let Some(mut s) = Session::start(true) else { return };
    hello(&mut s);
    assert!(s.wait_event(|v| v["event"] == "state" && v["state"] == "connected").is_some(), "not connected");
    let chat = std::env::var("FILI_IM_CONFORMANCE_CHAT").unwrap_or_else(|_| "paul".into());
    let params = json!({"chat": chat, "client_id": "conformance-1", "text": {"plain": "fili-im conformance test"}});
    let first = s.command("send", params.clone());
    assert_eq!(first["ok"], true, "send failed: {first}");
    assert!(first["result"]["id"].as_str().is_some_and(|id| !id.is_empty()), "send gives the message id");
    assert!(first["result"]["timestamp"].as_i64().is_some(), "send gives the timestamp");
    let again = s.command("send", params);
    assert_eq!(again["result"]["id"], first["result"]["id"], "the same client_id must not send twice");
}

#[test]
fn missing_chats_are_not_found() {
    let Some(mut s) = Session::start(true) else { return };
    hello(&mut s);
    assert!(s.wait_event(|v| v["event"] == "state" && v["state"] == "connected").is_some(), "not connected");
    let reply = s.command("send", json!({"chat": "no-such-chat-fili-im", "client_id": "c-x", "text": {"plain": "x"}}));
    assert_eq!(reply["ok"], false);
    assert!(matches!(reply["error"]["code"].as_str(), Some("not_found" | "invalid")), "got {reply}");
}
