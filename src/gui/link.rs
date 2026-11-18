//! The window's connection to the daemon: one Unix socket, a reader thread that survives daemon restarts, requests
//! written from the Qt thread. The window never touches the store; everything goes through the control socket.
use crate::paths;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::time::Duration;

/// What the reader thread hands to the Qt side.
pub enum Incoming {
    /// Connected (true) or lost (false) the daemon.
    Link(bool),
    /// The answer to request `id`: `ok` and the result or error as JSON.
    Reply { id: i32, ok: bool, body: String },
    /// A pushed event, as the daemon sent it.
    Event(String),
}

pub struct Link {
    writer: Mutex<Option<UnixStream>>,
    next: AtomicI32,
}

pub fn link() -> &'static Link {
    static LINK: Link = Link { writer: Mutex::new(None), next: AtomicI32::new(1) };
    &LINK
}

impl Link {
    /// Send a request; the id comes back in `Incoming::Reply`. `Err` when the daemon is not connected.
    pub fn call(&self, method: &str, params: &str) -> (i32, Result<(), ()>) {
        let id = self.next.fetch_add(1, Ordering::Relaxed).max(1);
        let params: Value = serde_json::from_str(params).unwrap_or(json!({}));
        let mut line = json!({"id": id, "method": method, "params": params}).to_string();
        line.push('\n');
        let mut writer = self.writer.lock().unwrap();
        let sent = match writer.as_mut() {
            Some(stream) => stream.write_all(line.as_bytes()).map_err(|_| ()),
            None => Err(()),
        };
        (id, sent)
    }
}

/// Set by [`restart`] until a new daemon answers.
static RESTART: AtomicBool = AtomicBool::new(false);

/// Restart the daemon (View > Restart Service): it is asked to quit, and the reader thread starts a new one as soon as
/// the old one is gone, then reconnects. Starts it too when none is running.
pub fn restart() {
    RESTART.store(true, Ordering::SeqCst);
    let _ = link().call("shutdown", "{}");
}

/// Start the daemon when none answers (it holds an instance lock, so a second one exits at once).
fn start_daemon() {
    let Ok(exe) = std::env::current_exe() else { return };
    let _ = std::fs::create_dir_all(paths::state_dir());
    let log = std::fs::OpenOptions::new().create(true).append(true).open(paths::state_dir().join("daemon.log"));
    let mut command = std::process::Command::new(exe);
    command.arg("daemon").stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null());
    if let Ok(log) = log {
        command.stderr(log);
    }
    // Own process group: closing the window or its terminal must not take the daemon with it.
    command.process_group(0);
    let _ = command.spawn();
}

/// Connect, subscribe and read until the daemon goes away, then start again. Blocks: run it in a thread.
pub fn run(deliver: impl Fn(Incoming) + Send + 'static) {
    let mut started = false;
    let mut tries = 0u32;
    loop {
        let stream = match UnixStream::connect(paths::socket()) {
            Ok(stream) => stream,
            Err(_) => {
                // After a restart, the old daemon holds its instance lock while its connectors stop (a few seconds):
                // a new one is tried every 2 s, one started too early exits at once
                let restarting = RESTART.load(Ordering::SeqCst);
                if !started || (restarting && tries.is_multiple_of(4)) {
                    started = true;
                    start_daemon();
                }
                tries += 1;
                std::thread::sleep(Duration::from_millis(500));
                continue;
            }
        };
        let Ok(read) = stream.try_clone() else { continue };
        let mut subscribe = stream.try_clone().expect("clone");
        let _ = subscribe.write_all(b"{\"id\":0,\"method\":\"subscribe\",\"params\":{}}\n");
        RESTART.store(false, Ordering::SeqCst);
        tries = 0;
        *link().writer.lock().unwrap() = Some(stream);
        deliver(Incoming::Link(true));
        for line in BufReader::new(read).lines() {
            let Ok(line) = line else { break };
            let Ok(value) = serde_json::from_str::<Value>(&line) else { continue };
            if value.get("event").is_some() {
                deliver(Incoming::Event(line));
            } else if let Some(id) = value.get("reply").and_then(Value::as_i64) {
                let ok = value["ok"].as_bool() == Some(true);
                let body = if ok { value["result"].to_string() } else { value["error"].to_string() };
                if id > 0 {
                    deliver(Incoming::Reply { id: id as i32, ok, body });
                }
            }
        }
        *link().writer.lock().unwrap() = None;
        deliver(Incoming::Link(false));
        std::thread::sleep(Duration::from_millis(500));
    }
}
