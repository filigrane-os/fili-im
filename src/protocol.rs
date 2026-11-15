//! Connector protocol, version 1: the Rust side of `docs/connector-protocol.md`.
//!
//! Lines are told apart by their first key: `event` (connector → daemon), `cmd` (daemon → connector) and `reply`
//! (connector → daemon). Unknown keys are ignored; unknown events are skipped, never fatal.
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const PROTOCOL: u32 = 1;

// ---------------------------------------------------------------------------------------------------------------
// Text

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Text {
    pub plain: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spans: Vec<Span>,
}

impl Text {
    pub fn plain(text: impl Into<String>) -> Self {
        Text { plain: text.into(), spans: Vec::new() }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub style: Style,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contact: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Style {
    Bold,
    Italic,
    Strike,
    Code,
    Pre,
    Link,
    Mention,
    /// A style from a newer protocol revision: shown as plain text.
    #[serde(other)]
    Unknown,
}

// ---------------------------------------------------------------------------------------------------------------
// Capabilities

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Capabilities {
    pub pairing: Vec<String>,
    pub message_kinds: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edit: Option<Limit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delete: Option<Limit>,
    pub reactions: Reactions,
    pub replies: bool,
    pub threads: bool,
    pub receipts: Receipts,
    pub typing: Typing,
    pub history: History,
    pub schedule_native: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_max_bytes: Option<u64>,
    pub chat_kinds: Vec<String>,
    pub formatting: Formatting,
    pub identity: Identity,
}

impl Default for Capabilities {
    fn default() -> Self {
        Capabilities {
            pairing: Vec::new(),
            message_kinds: vec!["text".into()],
            edit: None,
            delete: None,
            reactions: Reactions::None,
            replies: false,
            threads: false,
            receipts: Receipts::default(),
            typing: Typing::default(),
            history: History::None,
            schedule_native: false,
            media_max_bytes: None,
            chat_kinds: vec!["direct".into()],
            formatting: Formatting::Plain,
            identity: Identity::default(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Limit {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_s: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Reactions {
    #[default]
    None,
    OnePerUser,
    Many,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum History {
    #[default]
    None,
    OnPairing,
    OnDemand,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Formatting {
    #[default]
    Plain,
    Spans,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(default)]
pub struct Receipts {
    pub send: bool,
    pub suppress: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(default)]
pub struct Typing {
    pub send: bool,
    pub receive: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(default)]
pub struct Identity {
    pub phone: bool,
}

impl Capabilities {
    /// Whether the connector may receive this command, given its parameters. Capabilities are a ceiling: the daemon
    /// refuses before sending, so a hidden button's shortcut cannot slip through.
    pub fn allows(&self, cmd: &str, params: &Map<String, Value>) -> Result<(), String> {
        let refuse = |what: &str| Err(format!("{what} is not available on this platform"));
        match cmd {
            "pair" => {
                let method = params.get("method").and_then(Value::as_str).unwrap_or("");
                if self.pairing.iter().any(|m| m == method) { Ok(()) } else { refuse(&format!("pairing by {method:?}")) }
            }
            "send" => {
                if params.get("reply_to").is_some_and(|v| !v.is_null()) && !self.replies {
                    return refuse("replying to a message");
                }
                let kind = params.get("media").and_then(|m| m.get("kind")).and_then(Value::as_str).unwrap_or("text");
                if self.message_kinds.iter().any(|k| k == kind) { Ok(()) } else { refuse(&format!("sending {kind}")) }
            }
            "edit" if self.edit.is_none() => refuse("editing"),
            "delete" if self.delete.is_none() => refuse("deleting"),
            "react" if self.reactions == Reactions::None => refuse("reacting"),
            "mark_read" if !self.receipts.send => refuse("read receipts"),
            "typing" if !self.typing.send => refuse("typing indicators"),
            "backfill" if self.history != History::OnDemand => refuse("loading older messages"),
            _ => Ok(()),
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Objects carried by events

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SelfInfo {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Chat {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unread: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub muted_until: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_read: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Contact {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Media {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(rename = "ref", default)]
    pub reference: String,
    /// Outgoing only: the local file to send (and, once sent, the copy the window shows).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

fn text_kind() -> String {
    "text".into()
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Message {
    pub id: String,
    pub chat: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sender: Option<String>,
    #[serde(default)]
    pub from_me: bool,
    pub timestamp: i64,
    #[serde(default = "text_kind")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<Text>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<Media>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnState {
    Connecting,
    Connected,
    NeedsPairing,
    LoggedOut,
    Banned,
    PhoneOffline,
    Error,
}

impl ConnState {
    pub fn as_str(self) -> &'static str {
        match self {
            ConnState::Connecting => "connecting",
            ConnState::Connected => "connected",
            ConnState::NeedsPairing => "needs_pairing",
            ConnState::LoggedOut => "logged_out",
            ConnState::Banned => "banned",
            ConnState::PhoneOffline => "phone_offline",
            ConnState::Error => "error",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptKind {
    Delivered,
    Read,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Progress {
    pub done: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
}

// ---------------------------------------------------------------------------------------------------------------
// Events

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "event")]
pub enum Event {
    #[serde(rename = "hello")]
    Hello {
        protocol: u32,
        platform: String,
        #[serde(default)]
        name: String,
        #[serde(default)]
        version: String,
        #[serde(default)]
        capabilities: Capabilities,
    },
    #[serde(rename = "state")]
    State {
        state: ConnState,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
        #[serde(rename = "self", default, skip_serializing_if = "Option::is_none")]
        me: Option<SelfInfo>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        logout_in_s: Option<u64>,
    },
    #[serde(rename = "pairing")]
    Pairing {
        kind: String,
        data: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expires_s: Option<u64>,
    },
    #[serde(rename = "chat.upsert")]
    ChatUpsert { chat: Chat },
    #[serde(rename = "contact.upsert")]
    ContactUpsert { contact: Contact },
    #[serde(rename = "message.new")]
    MessageNew {
        #[serde(default)]
        live: bool,
        message: Message,
    },
    #[serde(rename = "history.batch")]
    HistoryBatch {
        messages: Vec<Message>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        progress: Option<Progress>,
    },
    #[serde(rename = "message.edit")]
    MessageEdit {
        chat: String,
        id: String,
        text: Text,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timestamp: Option<i64>,
    },
    #[serde(rename = "message.delete")]
    MessageDelete { chat: String, id: String },
    #[serde(rename = "reaction")]
    Reaction {
        chat: String,
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sender: Option<String>,
        #[serde(default)]
        from_me: bool,
        emoji: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timestamp: Option<i64>,
    },
    #[serde(rename = "receipt")]
    Receipt {
        chat: String,
        messages: Vec<String>,
        kind: ReceiptKind,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sender: Option<String>,
        #[serde(default)]
        from_me: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timestamp: Option<i64>,
    },
    #[serde(rename = "typing")]
    Typing {
        chat: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sender: Option<String>,
        active: bool,
    },
    #[serde(rename = "error")]
    Error { message: String },
}

const KNOWN_EVENTS: &[&str] = &[
    "hello",
    "state",
    "pairing",
    "chat.upsert",
    "contact.upsert",
    "message.new",
    "history.batch",
    "message.edit",
    "message.delete",
    "reaction",
    "receipt",
    "typing",
    "error",
];

// ---------------------------------------------------------------------------------------------------------------
// Commands and replies

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Command {
    pub id: u64,
    pub cmd: String,
    #[serde(flatten)]
    pub params: Map<String, Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ErrorBody {
    pub code: String,
    #[serde(default)]
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_after_s: Option<u64>,
}

impl ErrorBody {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        ErrorBody { code: code.into(), message: message.into(), retry_after_s: None }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Reply {
    pub reply: u64,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorBody>,
}

impl Reply {
    pub fn ok(id: u64, result: Value) -> Self {
        Reply { reply: id, ok: true, result: Some(result), error: None }
    }
    pub fn err(id: u64, code: &str, message: impl Into<String>) -> Self {
        Reply { reply: id, ok: false, result: None, error: Some(ErrorBody::new(code, message)) }
    }
}

/// One line from a connector.
#[derive(Debug)]
pub enum Incoming {
    Event(Box<Event>),
    Reply(Reply),
    /// An event from a newer protocol revision: skipped.
    Unknown(String),
    /// Not a protocol line at all.
    Invalid(String),
}

pub fn parse_line(line: &str) -> Incoming {
    let value: Value = match serde_json::from_str(line) {
        Ok(Value::Object(map)) => Value::Object(map),
        Ok(_) => return Incoming::Invalid("not a JSON object".into()),
        Err(e) => return Incoming::Invalid(e.to_string()),
    };
    if value.get("reply").is_some() {
        return match serde_json::from_value(value) {
            Ok(reply) => Incoming::Reply(reply),
            Err(e) => Incoming::Invalid(format!("bad reply: {e}")),
        };
    }
    let Some(name) = value.get("event").and_then(Value::as_str).map(str::to_owned) else {
        return Incoming::Invalid("neither an event nor a reply".into());
    };
    if !KNOWN_EVENTS.contains(&name.as_str()) {
        return Incoming::Unknown(name);
    }
    match serde_json::from_value(value) {
        Ok(event) => Incoming::Event(Box::new(event)),
        Err(e) => Incoming::Invalid(format!("bad {name} event: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn hello_with_partial_capabilities_takes_the_defaults() {
        let Incoming::Event(event) =
            parse_line(r#"{"event":"hello","protocol":1,"platform":"x","capabilities":{"replies":true,"future":1}}"#)
        else {
            panic!("not an event")
        };
        let Event::Hello { capabilities, .. } = *event else { panic!("not hello") };
        assert!(capabilities.replies);
        assert_eq!(capabilities.message_kinds, vec!["text"]);
        assert_eq!(capabilities.chat_kinds, vec!["direct"]);
        assert!(capabilities.edit.is_none());
    }

    #[test]
    fn unknown_events_are_skipped_and_garbage_is_invalid() {
        assert!(matches!(parse_line(r#"{"event":"story.new","x":1}"#), Incoming::Unknown(name) if name == "story.new"));
        assert!(matches!(parse_line("[1]"), Incoming::Invalid(_)));
        assert!(matches!(parse_line("hello"), Incoming::Invalid(_)));
        assert!(matches!(parse_line(r#"{"x":1}"#), Incoming::Invalid(_)));
    }

    #[test]
    fn replies_and_events_round_trip() {
        let reply = Reply::err(3, "unsupported", "nope");
        let line = serde_json::to_string(&reply).unwrap();
        assert!(matches!(parse_line(&line), Incoming::Reply(r) if r == reply));
        let event = Event::State { state: ConnState::Connected, detail: None, me: None, logout_in_s: None };
        assert_eq!(serde_json::to_value(&event).unwrap(), json!({"event":"state","state":"connected"}));
    }

    #[test]
    fn unknown_span_styles_survive() {
        let text: Text = serde_json::from_value(json!({"plain":"ab","spans":[{"start":0,"end":1,"style":"spoiler"}]})).unwrap();
        assert_eq!(text.spans[0].style, Style::Unknown);
    }

    #[test]
    fn capabilities_are_a_ceiling() {
        let caps = Capabilities::default();
        let none = Map::new();
        assert!(caps.allows("send", &none).is_ok());
        assert!(caps.allows("edit", &none).is_err());
        assert!(caps.allows("mark_read", &none).is_err());
        assert!(caps.allows("react", &none).is_err());
        let image = json!({"media":{"kind":"image"}}).as_object().unwrap().clone();
        assert!(caps.allows("send", &image).is_err());
        let reply = json!({"reply_to":"x"}).as_object().unwrap().clone();
        assert!(caps.allows("send", &reply).is_err());
        let qr = json!({"method":"qr"}).as_object().unwrap().clone();
        assert!(caps.allows("pair", &qr).is_err());
        let full = Capabilities {
            pairing: vec!["qr".into()],
            replies: true,
            edit: Some(Limit::default()),
            receipts: Receipts { send: true, suppress: true },
            ..Default::default()
        };
        assert!(full.allows("pair", &qr).is_ok());
        assert!(full.allows("send", &reply).is_ok());
        assert!(full.allows("edit", &none).is_ok());
        assert!(full.allows("mark_read", &none).is_ok());
        assert!(full.allows("whatever_new", &none).is_ok());
    }
}
