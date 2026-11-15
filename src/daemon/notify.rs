//! Desktop notifications (`notify-send`) and the fili-bar's alert files.
use crate::paths;
use std::collections::VecDeque;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Messages kept for the bar's tooltip.
pub const RECENT: i64 = 8;
/// At most this many notifications in `BURST_WINDOW`; one summary after that, then silence until the window clears.
const BURST: usize = 4;
const BURST_WINDOW: Duration = Duration::from_secs(10);
const BODY_LIMIT: usize = 200;

pub struct Notifier {
    enabled: bool,
    recent: Mutex<VecDeque<Instant>>,
}

enum Verdict {
    Show,
    Summary,
    Quiet,
}

impl Notifier {
    pub fn new(enabled: bool) -> Self {
        Notifier { enabled, recent: Mutex::new(VecDeque::new()) }
    }

    fn verdict(&self, now: Instant) -> Verdict {
        let mut recent = self.recent.lock().unwrap();
        while recent.front().is_some_and(|t| now.duration_since(*t) > BURST_WINDOW) {
            recent.pop_front();
        }
        recent.push_back(now);
        match recent.len() {
            n if n < BURST => Verdict::Show,
            n if n == BURST => Verdict::Summary,
            _ => Verdict::Quiet,
        }
    }

    pub fn announce(&self, title: &str, body: &str) {
        if !self.enabled {
            return;
        }
        let (title, body) = match self.verdict(Instant::now()) {
            Verdict::Show => (title.to_string(), clip(body)),
            Verdict::Summary => ("Conversations".to_string(), "More new messages".to_string()),
            Verdict::Quiet => return,
        };
        tokio::spawn(async move {
            let _ = tokio::process::Command::new("notify-send")
                .args(["--app-name=Conversations", "--icon=fili-im", "--category=im.received", "--expire-time=8000", "--"])
                .arg(title)
                .arg(body)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .await;
        });
    }
}

fn clip(text: &str) -> String {
    let flat = text.replace(['\n', '\r'], " ");
    if flat.chars().count() <= BODY_LIMIT {
        return flat;
    }
    let mut out: String = flat.chars().take(BODY_LIMIT - 1).collect();
    out.push('…');
    out
}

/// `~/.local/state/alerts/messages` (one `title<TAB>line` per recent unread message) and `messages.count` (the
/// unread total, which wins): the files the bar's `messages` module reads. Written only when they change.
/// (unread total, tooltip lines) last written.
type Written = (i64, Vec<(String, String)>);

#[derive(Default)]
pub struct Alerts {
    last: Mutex<Option<Written>>,
}

impl Alerts {
    pub fn write(&self, total: i64, recent: &[(String, String)]) {
        let mut last = self.last.lock().unwrap();
        if last.as_ref().is_some_and(|(t, r)| *t == total && r == recent) {
            return;
        }
        write_in(&paths::alerts_dir(), total, recent);
        *last = Some((total, recent.to_vec()));
    }
}

fn write_in(dir: &Path, total: i64, recent: &[(String, String)]) {
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    let clean = |text: &str| text.replace(['\t', '\n', '\r'], " ");
    let lines: String = recent.iter().map(|(title, line)| format!("{}\t{}\n", clean(title), clean(line))).collect();
    for (name, body) in [("messages", lines), ("messages.count", format!("{total}\n"))] {
        let temporary = dir.join(format!(".{name}.tmp"));
        if std::fs::write(&temporary, body).is_ok() {
            let _ = std::fs::rename(&temporary, dir.join(name));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bursts_become_one_summary() {
        let notifier = Notifier::new(true);
        let start = Instant::now();
        let verdicts: Vec<&str> = (0..6)
            .map(|i| match notifier.verdict(start + Duration::from_millis(i * 100)) {
                Verdict::Show => "show",
                Verdict::Summary => "summary",
                Verdict::Quiet => "quiet",
            })
            .collect();
        assert_eq!(verdicts, ["show", "show", "show", "summary", "quiet", "quiet"]);
        assert!(matches!(notifier.verdict(start + Duration::from_secs(30)), Verdict::Show));
    }

    #[test]
    fn long_bodies_are_clipped_on_one_line() {
        assert_eq!(clip("a\nb"), "a b");
        let long = "é".repeat(500);
        assert_eq!(clip(&long).chars().count(), BODY_LIMIT);
    }

    #[test]
    fn alert_files_hold_recent_lines_and_the_total() {
        let dir = std::env::temp_dir().join(format!("fili-im-alerts-{}", std::process::id()));
        write_in(&dir, 3, &[("Paul\tM.".into(), "On mange\noù ?".into())]);
        assert_eq!(std::fs::read_to_string(dir.join("messages")).unwrap(), "Paul M.\tOn mange où ?\n");
        assert_eq!(std::fs::read_to_string(dir.join("messages.count")).unwrap(), "3\n");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
