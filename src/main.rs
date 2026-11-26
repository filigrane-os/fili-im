//! `fili-im`: Conversations. `daemon` runs the daemon; the other subcommands talk to it. Without a subcommand it opens
//! the window; `ctl` commands an open window (the fili-bar menu).
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
        /// Only conversations whose title contains TEXT (case and accents ignored)
        #[arg(long)]
        find: Option<String>,
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
    /// Send a text message. TARGET is a conversation number, or a part of its title ("fili-im send Anna hello"): one match
    /// sends, several list the candidates and send nothing
    Send { target: String, text: String },
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
    /// Show the window, on a conversation if given (starts it when it is not open)
    Open { chat: Option<i64> },
    /// Print the daemon's events as they come (JSON lines)
    Watch,
}

fn main() {
    // No argument: the window. `ctl` commands an open window and never starts Qt.
    #[cfg(feature = "gui")]
    if std::env::args().skip(1).all(|a| fili_im::window_parts::is_flag(&a)) {
        if fili_im::ctl::focus_existing() {
            return;
        }
        std::process::exit(fili_im::gui::run());
    }
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).is_some_and(|a| a == "ctl") {
        match fili_im::ctl::run(&args[2..]) {
            Ok(answer) => println!("{answer}"),
            Err(e) => {
                eprintln!("fili-im ctl: {e}");
                std::process::exit(1);
            }
        }
        return;
    }
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

/// Lowercase without accents, to find a conversation by a part of its title
fn fold(s: &str) -> String {
    s.chars()
        .flat_map(|c| c.to_lowercase())
        .map(|c| match c {
            'à' | 'â' | 'ä' | 'á' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' | 'í' => 'i',
            'ô' | 'ö' | 'ó' => 'o',
            'ù' | 'û' | 'ü' | 'ú' => 'u',
            'ç' => 'c',
            c => c,
        })
        .collect()
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
    if let Some(Cmd::Open { chat }) = cli.command {
        // No daemon needed: this only drives the window
        fili_im::ctl::show_window(&chat.map(|c| vec![format!("open-chat:{c}")]).unwrap_or_default());
        return Ok(());
    }
    let mut client = Client::connect()?;
    let j = cli.json;
    match cli.command.unwrap_or(Cmd::Status) {
        Cmd::Daemon | Cmd::Open { .. } => unreachable!(),
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
        Cmd::Chats { find, account, unread, limit } => {
            let mut chats = client.call(
                "chats.list",
                json!({"account": account, "unread": unread, "limit": if find.is_some() { 1000 } else { limit }}),
            )?;
            if let (Some(find), Some(list)) = (&find, chats.as_array_mut()) {
                let want = fold(find);
                list.retain(|c| fold(c["title"].as_str().unwrap_or("")).contains(&want));
                list.truncate(limit.max(0) as usize);
            }
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
        Cmd::Send { target, text } => {
            let (chat, title) = match target.parse::<i64>() {
                Ok(id) => (id, None),
                Err(_) => {
                    let all = client.call("chats.list", json!({"limit": 1000}))?;
                    let want = fold(&target);
                    let hits: Vec<&Value> = all
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter(|c| fold(c["title"].as_str().unwrap_or("")).contains(&want))
                        .collect();
                    match hits.as_slice() {
                        [] => anyhow::bail!("no conversation matches {target:?}"),
                        [one] => (one["id"].as_i64().unwrap_or(0), one["title"].as_str().map(str::to_string)),
                        many => {
                            let list: Vec<String> =
                                many.iter().map(|c| format!("{:>5}  {}", c["id"], c["title"].as_str().unwrap_or(""))).collect();
                            anyhow::bail!(
                                "{} conversations match {target:?}, nothing sent; use the number:\n{}",
                                many.len(),
                                list.join("\n")
                            )
                        }
                    }
                }
            };
            let message = client.call("send", json!({"chat": chat, "text": text}))?;
            print(j, &message, |_| match &title {
                Some(t) => format!("Sent to {t} ({chat})"),
                None => format!("Sent to conversation {chat}"),
            });
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
