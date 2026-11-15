//! Read receipts policy (`[receipts]` of `~/.config/filigrane/fili-im.toml`): whether opening a conversation tells the
//! sender "read". Peeking from the bar or a notification never does.
//!
//! ```toml
//! [receipts]
//! default = "on_open"      # on_open | manual | never
//! whatsapp-1 = "manual"    # an account
//! chat:42 = "never"        # one conversation (its id in `fili-im chats`)
//! ```
//! `on_open`: opening the conversation in the window sends receipts. `manual`: only the explicit "Mark as read"
//! does. `never`: no receipt is ever sent from here.
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Policy {
    OnOpen,
    Manual,
    Never,
}

impl Policy {
    fn parse(text: &str) -> Option<Policy> {
        match text {
            "on_open" => Some(Policy::OnOpen),
            "manual" => Some(Policy::Manual),
            "never" => Some(Policy::Never),
            _ => None,
        }
    }
}

/// What asks for the receipt: opening the conversation (`Auto`), or the user's explicit "mark as read".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ask {
    No,
    Auto,
    Explicit,
}

impl Policy {
    pub fn sends(self, ask: Ask) -> bool {
        match (self, ask) {
            (_, Ask::No) | (Policy::Never, _) => false,
            (Policy::OnOpen, _) => true,
            (Policy::Manual, ask) => ask == Ask::Explicit,
        }
    }
}

pub fn config_file() -> PathBuf {
    crate::window_parts::config_dir().join("fili-im.toml")
}

/// The policy of one conversation: its own line, else its account's, else `default`, else `on_open`.
pub fn receipts(account: &str, chat: i64) -> Policy {
    let text = std::fs::read_to_string(config_file()).unwrap_or_default();
    receipts_from(&text, account, chat)
}

pub fn receipts_from(text: &str, account: &str, chat: i64) -> Policy {
    let mut inside = false;
    let (mut own, mut acct, mut default) = (None, None, None);
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            inside = name.trim() == "receipts";
        } else if inside && let Some((key, value)) = line.split_once('=') {
            let (key, value) = (key.trim().trim_matches('"'), Policy::parse(value.trim().trim_matches('"')));
            if key == format!("chat:{chat}") {
                own = value;
            } else if key == account {
                acct = value;
            } else if key == "default" {
                default = value;
            }
        }
    }
    own.or(acct).or(default).unwrap_or(Policy::OnOpen)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_conversation_beats_its_account_beats_the_default() {
        let text =
            "[receipts]\ndefault = \"manual\"\nwa-1 = \"never\" # all of it\nchat:7 = \"on_open\"\n[other]\nchat:9 = \"never\"\n";
        assert_eq!(receipts_from(text, "wa-1", 7), Policy::OnOpen);
        assert_eq!(receipts_from(text, "wa-1", 8), Policy::Never);
        assert_eq!(receipts_from(text, "wa-2", 8), Policy::Manual);
        assert_eq!(receipts_from(text, "wa-2", 9), Policy::Manual, "other sections are ignored");
        assert_eq!(receipts_from("", "wa-1", 1), Policy::OnOpen);
        assert_eq!(receipts_from("[receipts]\ndefault = \"bogus\"\n", "a", 1), Policy::OnOpen);
    }

    #[test]
    fn who_sends_when() {
        assert!(Policy::OnOpen.sends(Ask::Auto) && Policy::OnOpen.sends(Ask::Explicit));
        assert!(!Policy::Manual.sends(Ask::Auto) && Policy::Manual.sends(Ask::Explicit));
        assert!(!Policy::Never.sends(Ask::Explicit) && !Policy::OnOpen.sends(Ask::No));
    }
}
