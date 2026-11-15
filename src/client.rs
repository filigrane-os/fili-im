//! A blocking client of the daemon's control socket, for the CLI (the window will use its own async one).
use crate::paths;
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;

pub struct Client {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
    next: u64,
}

/// An error answer from the daemon, kept apart so the CLI can show its code.
#[derive(Debug)]
pub struct Refused {
    pub code: String,
    pub message: String,
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.code)
    }
}

impl std::error::Error for Refused {}

impl Client {
    pub fn connect() -> Result<Client> {
        let path = paths::socket();
        // A daemon started a moment ago (`fili-im daemon & fili-im add …`) may not have opened its socket yet.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        let stream = loop {
            match UnixStream::connect(&path) {
                Ok(stream) => break stream,
                Err(_) if std::time::Instant::now() < deadline => std::thread::sleep(std::time::Duration::from_millis(100)),
                Err(e) => return Err(e).context("the daemon is not running (start it with: fili-im daemon)"),
            }
        };
        Ok(Client { reader: BufReader::new(stream.try_clone()?), writer: stream, next: 1 })
    }

    fn read_value(&mut self) -> Result<Value> {
        let mut line = String::new();
        if self.reader.read_line(&mut line)? == 0 {
            bail!("the daemon closed the connection");
        }
        Ok(serde_json::from_str(&line)?)
    }

    /// Call a method; events that arrive meanwhile (after `subscribe`) are dropped.
    pub fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next;
        self.next += 1;
        let mut line = json!({"id": id, "method": method, "params": params}).to_string();
        line.push('\n');
        self.writer.write_all(line.as_bytes())?;
        loop {
            let value = self.read_value()?;
            if value.get("reply").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if value["ok"].as_bool() == Some(true) {
                return Ok(value["result"].clone());
            }
            let error = &value["error"];
            return Err(Refused {
                code: error["code"].as_str().unwrap_or("failed").into(),
                message: error["message"].as_str().unwrap_or("").into(),
            }
            .into());
        }
    }

    /// The next pushed event (after `subscribe`), replies to nothing are skipped.
    pub fn next_event(&mut self) -> Result<Value> {
        loop {
            let value = self.read_value()?;
            if value.get("event").is_some() {
                return Ok(value);
            }
        }
    }
}
