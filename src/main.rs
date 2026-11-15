//! `fili-im`: Conversations. `daemon` runs the daemon; the other subcommands talk to it. Without a subcommand it will
//! open the window (phase P2); until then it shows the status.
use anyhow::Result;
use clap::{Parser, Subcommand};
use fili_im::client::Client;
use serde_json::{Value, json};

#[derive(Parser)]
#[command(name = "fili-im", version, about = "Conversations: instant messaging for Filigrane")]
struct Cli {
    /// Print JSON instead of text (for scripts and the AI agent)
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run the daemon (started with the session; one per session)
    Daemon,
    /// Accounts, their state and the unread total
    Status,
    /// Installed connectors
    Platforms,
    /// Add an account on a platform, then pair it
    Add { platform: String },
    /// Pair an account: shows the QR code data or the pairing code until it is connected
    Pair {
        account: String,
        /// Pair with a code instead of a QR code (WhatsApp: the phone number, international format)
        #[arg(long)]
        phone: Option<String>,
    },
    /// Remove an account: logs out and deletes its messages from this computer
    Remove { account: String },
    /// Conversations, pinned then most recent
    Chats {
        #[arg(long)]
        account: Option<String>,
        /// Only conversations with unread messages
        #[arg(long)]
        unread: bool,
        #[arg(long, default_value_t = 50)]
        limit: i64,
    },
    /// Messages of a conversation, oldest first
    Messages {
        chat: i64,
        #[arg(long, default_value_t = 30)]
        limit: i64,
    },
    /// Send a text message
    Send { chat: i64, text: String },
    /// Mark a conversation read here
    Read {
        chat: i64,
        /// Also send read receipts to the platform
        #[arg(long)]
        receipt: bool,
    },
    /// Search every conversation
    Search {
        query: String,
        #[arg(long, default_value_t = 30)]
        limit: i64,
    },
    /// Print the daemon's events as they come (JSON lines)
    Watch,
}

fn main() {
    let cli = Cli::parse();
    if matches!(cli.command, Some(Cmd::Daemon)) {
        std::process::exit(fili_im::daemon::run());
    }
    if let Err(e) = run(cli) {
        eprintln!("fili-im: {e:#}");
        std::process::exit(1);
    }
}

fn print(json_out: bool, value: &Value, text: impl FnOnce(&Value) -> String) {
    if json_out {
        println!("{}", serde_json::to_string_pretty(value).unwrap_or_default());
    } else {
        let out = text(value);
        if !out.is_empty() {
            println!("{out}");
        }
    }
}

fn time_of(ms: i64) -> String {
    // Local time without a date library: the C library knows the time zone.
    let secs = (ms / 1000) as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: localtime_r fills the struct it is given.
    if unsafe { libc::localtime_r(&secs, &mut tm) }.is_null() {
        return String::new();
    }
    format!("{:04}-{:02}-{:02} {:02}:{:02}", tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday, tm.tm_hour, tm.tm_min)
}

fn message_line(m: &Value) -> String {
    let who = if m["from_me"].as_bool() == Some(true) {
        "me".to_string()
    } else {
        m["sender_name"].as_str().or(m["sender"].as_str()).unwrap_or("?").to_string()
    };
    let text = if m["deleted"].as_bool() == Some(true) {
        "(deleted)".to_string()
    } else {
        let media = m["media"].as_object().map(|o| Value::Object(o.clone()).to_string());
        fili_im::store::preview(m["kind"].as_str().unwrap_or("text"), m["text"]["plain"].as_str().unwrap_or(""), media.as_deref())
    };
    let state = match m["state"].as_str() {
        Some("pending") => " (sending)",
        Some("failed") => " (not sent)",
        _ => "",
    };
    format!("{}  {}: {}{}", time_of(m["timestamp"].as_i64().unwrap_or(0)), who, text, state)
}

fn account_line(a: &Value) -> String {
    let who = a["self_name"].as_str().or(a["self_id"].as_str()).unwrap_or("");
    let detail = a["detail"].as_str().map(|d| format!(" ({d})")).unwrap_or_default();
    format!(
        "{:<14} {:<10} {:<14} {}{}",
        a["key"].as_str().unwrap_or(""),
        a["name"].as_str().unwrap_or(""),
        a["state"].as_str().unwrap_or(""),
        who,
        detail
    )
}

fn run(cli: Cli) -> Result<()> {
    let mut client = Client::connect()?;
    let j = cli.json;
    match cli.command.unwrap_or(Cmd::Status) {
        Cmd::Daemon => unreachable!(),
        Cmd::Status => {
            let status = client.call("status", json!({}))?;
            print(j, &status, |s| {
                let mut out = format!("{} unread", s["unread"]);
                for a in s["accounts"].as_array().into_iter().flatten() {
                    out.push('\n');
                    out.push_str(&account_line(a));
                }
                out
            });
        }
        Cmd::Platforms => {
            let list = client.call("platforms.list", json!({}))?;
            print(j, &list, |l| {
                l.as_array()
                    .into_iter()
                    .flatten()
                    .map(|p| format!("{}\t{}", p["platform"].as_str().unwrap_or(""), p["program"].as_str().unwrap_or("")))
                    .collect::<Vec<_>>()
                    .join("\n")
            });
        }
        Cmd::Add { platform } => {
            let account = client.call("account.add", json!({"platform": platform}))?;
            print(j, &account, |a| {
                format!(
                    "Added {}. Pair it with: fili-im pair {}",
                    a["key"].as_str().unwrap_or(""),
                    a["key"].as_str().unwrap_or("")
                )
            });
        }
        Cmd::Pair { account, phone } => pair(&mut client, &account, phone, j)?,
        Cmd::Remove { account } => {
            client.call("account.remove", json!({"account": account}))?;
            print(j, &json!({}), |_| format!("Removed {account}"));
        }
        Cmd::Chats { account, unread, limit } => {
            let chats = client.call("chats.list", json!({"account": account, "unread": unread, "limit": limit}))?;
            print(j, &chats, |c| {
                c.as_array()
                    .into_iter()
                    .flatten()
                    .map(|c| {
                        let unread = c["unread"].as_i64().unwrap_or(0);
                        let badge = if unread > 0 { format!(" ({unread})") } else { String::new() };
                        let last =
                            if c["last"].is_null() { String::new() } else { format!("\n      {}", message_line(&c["last"])) };
                        format!(
                            "{:>5}  {}{}  [{}]{}",
                            c["id"],
                            c["title"].as_str().unwrap_or(""),
                            badge,
                            c["account"].as_str().unwrap_or(""),
                            last
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            });
        }
        Cmd::Messages { chat, limit } => {
            let messages = client.call("messages.list", json!({"chat": chat, "limit": limit}))?;
            print(j, &messages, |m| m.as_array().into_iter().flatten().map(message_line).collect::<Vec<_>>().join("\n"));
        }
        Cmd::Send { chat, text } => {
            let message = client.call("send", json!({"chat": chat, "text": text}))?;
            print(j, &message, |_| String::new());
        }
        Cmd::Read { chat, receipt } => {
            let result = client.call("mark_read", json!({"chat": chat, "receipt": receipt}))?;
            print(j, &result, |_| String::new());
        }
        Cmd::Search { query, limit } => {
            let hits = client.call("search", json!({"query": query, "limit": limit}))?;
            print(j, &hits, |h| {
                h.as_array()
                    .into_iter()
                    .flatten()
                    .map(|m| format!("{:>5}  {}  {}", m["chat"], m["chat_title"].as_str().unwrap_or(""), message_line(m)))
                    .collect::<Vec<_>>()
                    .join("\n")
            });
        }
        Cmd::Watch => {
            client.call("subscribe", json!({}))?;
            loop {
                println!("{}", client.next_event()?);
            }
        }
    }
    Ok(())
}

/// Ask for pairing and show what the phone needs until the account is connected.
fn pair(client: &mut Client, account: &str, phone: Option<String>, json_out: bool) -> Result<()> {
    client.call("subscribe", json!({}))?;
    let mut params = json!({"account": account});
    if let Some(phone) = phone {
        params["method"] = json!("code");
        params["phone"] = json!(phone);
    }
    client.call("account.pair", params)?;
    loop {
        let event = client.next_event()?;
        if json_out {
            println!("{event}");
        }
        match event["event"].as_str() {
            Some("pairing") if event["account"] == account && !json_out => match event["kind"].as_str() {
                Some("code") => println!("Pairing code: {}", event["data"].as_str().unwrap_or("")),
                _ => {
                    // Clear the screen: a new code replaces the old one every 20 seconds or so.
                    print!("\x1b[2J\x1b[H");
                    println!("{}", qr_text(event["data"].as_str().unwrap_or("")));
                    println!("Scan with the phone: WhatsApp › Settings › Linked devices › Link a device");
                }
            },
            Some("account") if event["account"]["key"] == account => match event["account"]["state"].as_str() {
                Some("connected") => {
                    if !json_out {
                        println!("Connected.");
                    }
                    return Ok(());
                }
                Some("logged_out" | "banned") => {
                    anyhow::bail!("{}", event["account"]["detail"].as_str().unwrap_or("pairing failed"))
                }
                _ => {}
            },
            _ => {}
        }
    }
}

/// A QR code drawn with half blocks, dark on light (phones need the quiet zone and the contrast).
fn qr_text(data: &str) -> String {
    match qrcode::QrCode::new(data.as_bytes()) {
        Ok(code) => code
            .render::<qrcode::render::unicode::Dense1x2>()
            .dark_color(qrcode::render::unicode::Dense1x2::Light)
            .light_color(qrcode::render::unicode::Dense1x2::Dark)
            .quiet_zone(true)
            .build(),
        Err(_) => data.to_string(),
    }
}
