//! One connector process per account: started, watched, restarted with back-off, stopped.
//!
//! Lines from the connector are handled in order by the supervisor task; replies are routed to the waiting command.
//! Anything that sends a command from an event (the outbox after `connected`) runs in its own task, because the
//! supervisor is the one reading the reply.
use super::Daemon;
use crate::paths;
use crate::protocol::{Capabilities, Command, ConnState, ErrorBody, Incoming, Reply, parse_line};
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin};
use tokio::sync::{oneshot, watch};

const LOG_LIMIT: u64 = 1 << 20;
const MAX_BACKOFF: u64 = 60;
/// A connector that stayed up this long starts its back-off again from one second.
/// A connector that stays `connecting` this long is killed and restarted (a hung socket does not recover by itself).
const STUCK: Duration = Duration::from_secs(150);
const STABLE: Duration = Duration::from_secs(60);

pub struct Connector {
    pub key: String,
    pub platform: String,
    stdin: tokio::sync::Mutex<Option<ChildStdin>>,
    pending: Mutex<HashMap<u64, oneshot::Sender<Reply>>>,
    next: AtomicU64,
    /// Set by `hello`, cleared when the process ends: no command goes out before it.
    caps: Mutex<Option<Capabilities>>,
    connected: AtomicBool,
    /// Last reported state and when it began: a connector stuck in `connecting` is restarted (see `STUCK`).
    phase: Mutex<(ConnState, Instant)>,
    stop: watch::Sender<bool>,
    /// One outbox flush at a time per account.
    pub flush: tokio::sync::Mutex<()>,
}

impl Connector {
    pub fn new(key: &str, platform: &str) -> Arc<Connector> {
        Arc::new(Connector {
            key: key.into(),
            platform: platform.into(),
            stdin: tokio::sync::Mutex::new(None),
            pending: Mutex::new(HashMap::new()),
            next: AtomicU64::new(1),
            caps: Mutex::new(None),
            connected: AtomicBool::new(false),
            phase: Mutex::new((ConnState::Connecting, Instant::now())),
            stop: watch::channel(false).0,
            flush: tokio::sync::Mutex::new(()),
        })
    }

    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    pub fn set_phase(&self, state: ConnState) {
        let mut phase = self.phase.lock().unwrap();
        if phase.0 != state {
            *phase = (state, Instant::now());
        }
    }

    fn stuck(&self) -> bool {
        let phase = self.phase.lock().unwrap();
        phase.0 == ConnState::Connecting && phase.1.elapsed() > STUCK
    }

    pub fn set_connected(&self, connected: bool) {
        self.connected.store(connected, Ordering::SeqCst);
    }

    pub fn set_capabilities(&self, caps: Option<Capabilities>) {
        *self.caps.lock().unwrap() = caps;
    }

    pub fn stop(&self) {
        self.stop.send_replace(true);
    }

    fn stopped(&self) -> bool {
        *self.stop.borrow()
    }

    /// Send a command and wait for its reply. Refused here when the process is not ready or the capability is absent.
    pub async fn command(&self, cmd: &str, params: Map<String, Value>, timeout: Duration) -> Result<Value, ErrorBody> {
        if params.contains_key("id") || params.contains_key("cmd") {
            return Err(ErrorBody::new("invalid", format!("{cmd}: parameters cannot be named id or cmd")));
        }
        // A connector just started (an account just added) has not said hello yet: give it a few seconds.
        for attempt in 0..80 {
            if self.caps.lock().unwrap().is_some() || self.stopped() {
                break;
            }
            if attempt < 79 {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
        {
            let caps = self.caps.lock().unwrap();
            let Some(caps) = caps.as_ref() else {
                return Err(ErrorBody::new("not_connected", "the connector is not running"));
            };
            caps.allows(cmd, &params).map_err(|m| ErrorBody::new("unsupported", m))?;
        }
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, tx);
        let mut line = serde_json::to_string(&Command { id, cmd: cmd.into(), params }).unwrap_or_default();
        line.push('\n');
        let written = {
            let mut stdin = self.stdin.lock().await;
            match stdin.as_mut() {
                Some(pipe) => pipe.write_all(line.as_bytes()).await.is_ok() && pipe.flush().await.is_ok(),
                None => false,
            }
        };
        if !written {
            self.pending.lock().unwrap().remove(&id);
            return Err(ErrorBody::new("not_connected", "the connector is not running"));
        }
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(reply)) if reply.ok => Ok(reply.result.unwrap_or(json!({}))),
            Ok(Ok(reply)) => Err(reply.error.unwrap_or_else(|| ErrorBody::new("failed", "no error given"))),
            Ok(Err(_)) => Err(ErrorBody::new("not_connected", "the connector stopped")),
            Err(_) => {
                self.pending.lock().unwrap().remove(&id);
                Err(ErrorBody::new("failed", format!("{cmd}: no answer from the connector")))
            }
        }
    }

    fn route(&self, reply: Reply) {
        if let Some(waiter) = self.pending.lock().unwrap().remove(&reply.reply) {
            let _ = waiter.send(reply);
        }
    }

    /// The process is gone: nobody will answer the commands in flight.
    async fn reset(&self) {
        *self.stdin.lock().await = None;
        self.set_capabilities(None);
        self.set_connected(false);
        self.pending.lock().unwrap().clear();
    }
}

fn open_log(key: &str) -> Stdio {
    let path = paths::connector_log(key);
    if paths::private_dir(&paths::state_dir()).is_err() {
        return Stdio::null();
    }
    let too_big = path.metadata().is_ok_and(|m| m.len() > LOG_LIMIT);
    let file = std::fs::OpenOptions::new().create(true).append(!too_big).write(true).truncate(too_big).open(&path);
    match file {
        Ok(file) => {
            use std::os::unix::fs::PermissionsExt;
            let _ = file.set_permissions(std::fs::Permissions::from_mode(0o600));
            Stdio::from(file)
        }
        Err(_) => Stdio::null(),
    }
}

/// Wait for `duration` or until the connector is stopped; true when stopped.
async fn pause(stop: &mut watch::Receiver<bool>, duration: Duration) -> bool {
    if *stop.borrow() {
        return true;
    }
    tokio::time::timeout(duration, stop.wait_for(|s| *s)).await.is_ok()
}

async fn end(child: &mut Child) {
    if tokio::time::timeout(Duration::from_secs(3), child.wait()).await.is_err() {
        let _ = child.kill().await;
    }
}

/// Run the account's connector until it is stopped.
pub async fn supervise(daemon: Arc<Daemon>, conn: Arc<Connector>) {
    let mut stop = conn.stop.subscribe();
    let mut backoff = 1;
    while !conn.stopped() {
        let Some(program) = paths::find_connector(&conn.platform) else {
            daemon.connector_down(
                &conn,
                "error",
                &format!("The {} connector (fili-im-{}) is not installed", conn.platform, conn.platform),
            );
            if pause(&mut stop, Duration::from_secs(MAX_BACKOFF)).await {
                break;
            }
            continue;
        };
        let dir = paths::account_dir(&conn.key);
        if let Err(e) = paths::private_dir(&dir) {
            daemon.connector_down(&conn, "error", &format!("Cannot create the account directory: {e}"));
            if pause(&mut stop, Duration::from_secs(MAX_BACKOFF)).await {
                break;
            }
            continue;
        }
        let spawned = tokio::process::Command::new(&program)
            .arg("--account")
            .arg(&conn.key)
            .arg("--data")
            .arg(&dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(open_log(&conn.key))
            .kill_on_drop(true)
            .spawn();
        let mut child = match spawned {
            Ok(child) => child,
            Err(e) => {
                daemon.connector_down(&conn, "error", &format!("Cannot start {}: {e}", program.display()));
                if pause(&mut stop, Duration::from_secs(backoff)).await {
                    break;
                }
                backoff = (backoff * 2).min(MAX_BACKOFF);
                continue;
            }
        };
        let started = Instant::now();
        *conn.stdin.lock().await = child.stdin.take();
        let mut lines = BufReader::new(child.stdout.take().expect("piped stdout")).lines();
        let mut stopping = false;
        *conn.phase.lock().unwrap() = (ConnState::Connecting, Instant::now());
        let mut watchdog = tokio::time::interval(Duration::from_secs(15));
        loop {
            tokio::select! {
                _ = watchdog.tick() => if conn.stuck() {
                    eprintln!("fili-im: {}: still connecting after {}s; restarting the connector", conn.key, STUCK.as_secs());
                    let _ = child.start_kill();
                    *conn.phase.lock().unwrap() = (ConnState::Connecting, Instant::now());
                },
                line = lines.next_line() => match line {
                    Ok(Some(line)) => match parse_line(&line) {
                        Incoming::Reply(reply) => conn.route(reply),
                        Incoming::Event(event) => {
                            if !daemon.on_event(&conn, *event) {
                                stopping = true;
                                break;
                            }
                        }
                        Incoming::Unknown(name) => daemon.debug(&format!("{}: skipped unknown event {name}", conn.key)),
                        // Never log the line itself: it may carry message contents.
                        Incoming::Invalid(why) => eprintln!("fili-im: {}: invalid line from the connector: {why}", conn.key),
                    },
                    _ => break,
                },
                _ = stop.wait_for(|s| *s) => {
                    stopping = true;
                    break;
                }
            }
        }
        if stopping {
            // Ask politely (nobody reads the reply any more: this task was the reader), close stdin, then end it.
            if let Some(mut pipe) = conn.stdin.lock().await.take() {
                let _ = pipe.write_all(b"{\"id\":0,\"cmd\":\"shutdown\"}\n").await;
                let _ = pipe.flush().await;
            }
            end(&mut child).await;
            conn.reset().await;
            break;
        }
        drop(conn.stdin.lock().await.take());
        let status = child.wait().await.ok();
        conn.reset().await;
        if conn.stopped() {
            break;
        }
        let how = status.map(|s| s.to_string()).unwrap_or_else(|| "unknown status".into());
        daemon.connector_down(&conn, "error", &format!("The connector stopped ({how}); restarting"));
        if started.elapsed() > STABLE {
            backoff = 1;
        }
        if pause(&mut stop, Duration::from_secs(backoff)).await {
            break;
        }
        backoff = (backoff * 2).min(MAX_BACKOFF);
    }
}
