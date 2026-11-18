// Object exposed to QML (module Fili.Im): the environment (live theme, translations, window parts) and the connection
// to the daemon (requests answered by signals, pushed events).
#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(bool, light, READ, NOTIFY = theme_changed)]
        #[qproperty(QString, accent, READ, NOTIFY = theme_changed)]
        // Counts the changes of the settings files (fili-im.toml, theme.toml): bindings that read it, and the header and
        // footer of the window (Main.qml, onThemeChanged), follow a change made while the window is open
        #[qproperty(i32, settings_rev, cxx_name = "settingsRev", READ, NOTIFY = theme_changed)]
        #[qproperty(bool, key_nav, cxx_name = "keyNav")]
        // Whether the daemon is connected
        #[qproperty(bool, online, READ, NOTIFY = online_changed)]
        type Conversations = super::ConversationsRust;

        #[qsignal]
        #[cxx_name = "themeChanged"]
        fn theme_changed(self: Pin<&mut Conversations>);
        #[qsignal]
        #[cxx_name = "onlineChanged"]
        fn online_changed(self: Pin<&mut Conversations>);

        // Merged theme (defaults of theme.toml + shared theme.toml + ~/.config/filigrane/fili-im.toml), as JSON
        #[qinvokable]
        #[cxx_name = "themeJson"]
        fn theme_json(self: &Conversations) -> QString;
        // Window management under niri (the window panel of the toolbar)
        #[qinvokable]
        #[cxx_name = "windowControl"]
        fn window_control(self: &Conversations) -> bool;
        #[qinvokable]
        #[cxx_name = "windowState"]
        fn window_state(self: &Conversations) -> QString;
        #[qinvokable]
        #[cxx_name = "windowAction"]
        fn window_action(self: &Conversations, name: &QString) -> bool;
        // Header and footer of the window: false = hidden ([window] header|footer, --no-header, --no-footer)
        #[qinvokable]
        #[cxx_name = "header"]
        fn header(self: &Conversations) -> bool;
        #[qinvokable]
        #[cxx_name = "footer"]
        fn footer(self: &Conversations) -> bool;
        // Interface state kept between runs (~/.local/state/filigrane/im/ui, lines key=value)
        #[qinvokable]
        #[cxx_name = "uiGet"]
        fn ui_get(self: &Conversations, key: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "uiSet"]
        fn ui_set(self: &Conversations, key: &QString, value: &QString);
        // The emoji list of the keyboard panel ("😀 grinning face (face-smiling)" per line), empty when missing
        #[qinvokable]
        fn emojis(self: &Conversations) -> QString;
        // Translation of a text written in English
        #[qinvokable]
        fn tr(self: &Conversations, text: &QString) -> QString;
        // Ask the daemon (method and JSON parameters); returns the request id, the answer comes as `replied`
        #[qinvokable]
        fn call(self: Pin<&mut Conversations>, method: &QString, params: &QString) -> i32;
        #[qsignal]
        fn replied(self: Pin<&mut Conversations>, id: i32, ok: bool, body: &QString);
        // A pushed event of the daemon (JSON, as sent)
        #[qsignal]
        fn pushed(self: Pin<&mut Conversations>, json: &QString);
        // A menu command from `fili-im ctl action NAME` (the fili-bar menu)
        #[qsignal]
        fn command(self: Pin<&mut Conversations>, name: &QString);
        // Rows of the About card: JSON [[label, value], ...]
        #[qinvokable]
        #[cxx_name = "aboutRows"]
        fn about_rows(self: &Conversations) -> QString;
        // Modules of a QR code as rows of 0 and 1 (JSON array of strings), for the pairing dialog
        #[qinvokable]
        #[cxx_name = "qrRows"]
        fn qr_rows(self: &Conversations, text: &QString) -> QString;
        // A new file name for a voice message being recorded, and the removal of one once it is sent or dropped
        #[qinvokable]
        #[cxx_name = "voicePath"]
        fn voice_path(self: &Conversations) -> QString;
        #[qinvokable]
        #[cxx_name = "discardVoice"]
        fn discard_voice(self: &Conversations, path: &QString);
        // Record the microphone (pw-record piped to opusenc: mono Ogg Opus at `path`); `voiceFinished` follows the stop
        #[qinvokable]
        #[cxx_name = "startVoice"]
        fn start_voice(self: Pin<&mut Conversations>, path: &QString) -> bool;
        #[qinvokable]
        #[cxx_name = "stopVoice"]
        fn stop_voice(self: Pin<&mut Conversations>);
        #[qsignal]
        #[cxx_name = "voiceFinished"]
        fn voice_finished(self: Pin<&mut Conversations>, path: &QString, ok: bool);
        // Open Graph data of a link (title, description, image, site) for a message: the answer comes as `linkPreview`
        #[qinvokable]
        #[cxx_name = "previewLink"]
        fn preview_link(self: Pin<&mut Conversations>, url: &QString) -> i32;
        #[qsignal]
        #[cxx_name = "linkPreview"]
        fn link_preview(self: Pin<&mut Conversations>, id: i32, json: &QString);
        // The address of a video page's stream (yt-dlp), for the player: the answer comes as `videoResolved`
        #[qinvokable]
        #[cxx_name = "resolveVideo"]
        fn resolve_video(self: Pin<&mut Conversations>, url: &QString) -> i32;
        #[qsignal]
        #[cxx_name = "videoResolved"]
        fn video_resolved(self: Pin<&mut Conversations>, id: i32, json: &QString);
        // [links] previews: "all" (default), "youtube" or "off"
        #[qinvokable]
        #[cxx_name = "linkMode"]
        fn link_mode(self: &Conversations) -> QString;
        // Play a video link (YouTube and the sites yt-dlp knows) in mpv; false when mpv is missing
        #[qinvokable]
        #[cxx_name = "playLink"]
        fn play_link(self: &Conversations, url: &QString) -> bool;
        // [links] previews in fili-im.toml: false = no request is made for a link ("off")
        #[qinvokable]
        #[cxx_name = "linkPreviews"]
        fn link_previews(self: &Conversations) -> bool;
        // Open a file or a link with the desktop (xdg-open)
        #[qinvokable]
        #[cxx_name = "openUrl"]
        fn open_url(self: &Conversations, url: &QString);
        // Files or a picture on the clipboard: JSON array of paths or file:// urls (a picture is saved first)
        #[qinvokable]
        #[cxx_name = "clipboardFiles"]
        fn clipboard_files(self: &Conversations) -> QString;
        // Put text on the clipboard of the desktop (wl-copy)
        #[qinvokable]
        #[cxx_name = "copyText"]
        fn copy_text(self: &Conversations, text: &QString);
        // Debug panel (src/logs.rs, qml/DebugPanel.qml, shared with the other fili-* applications): the log entries after
        // number `since` as JSON {"seq": last number, "entries": [[level, text], ...]} (level 0 debug, 1 info, 2 warning,
        // 3 error), the file of the log, a way to empty the panel, the file opened by its application, "Ask Rob" (records
        // an incident and asks Rob to look at it: its id, "" on failure), and uiState / setUiState, the names the shared
        // panel uses for uiGet / uiSet
        #[qinvokable]
        fn logs(self: &Conversations, since: i32) -> QString;
        #[qinvokable]
        #[cxx_name = "logFile"]
        fn log_file(self: &Conversations) -> QString;
        #[qinvokable]
        #[cxx_name = "clearLogs"]
        fn clear_logs(self: &Conversations);
        #[qinvokable]
        #[cxx_name = "openPath"]
        fn open_path(self: &Conversations, path: &QString);
        #[qinvokable]
        #[cxx_name = "reportProblem"]
        fn report_problem(self: &Conversations, text: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "uiState"]
        fn ui_state(self: &Conversations, key: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "setUiState"]
        fn set_ui_state(self: &Conversations, key: &QString, value: &QString);
    }

    impl cxx_qt::Initialize for Conversations {}
    impl cxx_qt::Threading for Conversations {}
}

use super::link::{self, Incoming};
use core::pin::Pin;
use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use qobject::*;
use serde_json::json;
use std::{env, path::PathBuf};

#[derive(Default)]
pub struct ConversationsRust {
    light: bool,
    accent: QString,
    settings_rev: i32,
    key_nav: bool,
    online: bool,
}

fn home() -> String {
    env::var("HOME").unwrap_or_default()
}

// Interface choices that outlive the window (layout of the window)
fn ui_file() -> PathBuf {
    let state = env::var("XDG_STATE_HOME").unwrap_or_else(|_| format!("{}/.local/state", home()));
    PathBuf::from(state).join("filigrane/im/ui")
}

// State of fili-theme ("light" or "dark"), like FiliTheme.qml of the fili-shell
fn theme_file() -> PathBuf {
    let state = env::var("XDG_STATE_HOME").unwrap_or_else(|_| format!("{}/.local/state", home()));
    PathBuf::from(state).join("filigrane/theme")
}

fn read_light() -> bool {
    std::fs::read_to_string(theme_file()).map(|t| t.trim() == "light").unwrap_or(false)
}

// Accent color (name), "default" by default
fn read_accent() -> String {
    std::fs::read_to_string(theme_file().with_file_name("accent"))
        .map(|t| t.trim().to_string())
        .unwrap_or_else(|_| "default".into())
}

/// Call `changed` each time something in `dir` is written (inotify, a thread asleep on read). Returns when the
/// directory cannot be watched or `changed` says the window is gone.
fn watch_dir(dir: PathBuf, mut changed: impl FnMut() -> bool + Send + 'static) {
    std::thread::spawn(move || {
        let _ = std::fs::create_dir_all(&dir);
        let Ok(c) = std::ffi::CString::new(dir.as_os_str().as_encoded_bytes()) else { return };
        // SAFETY: plain inotify calls on a descriptor owned by this thread.
        let fd = unsafe { libc::inotify_init1(libc::IN_CLOEXEC) };
        if fd < 0
            || unsafe { libc::inotify_add_watch(fd, c.as_ptr(), libc::IN_CLOSE_WRITE | libc::IN_MOVED_TO | libc::IN_CREATE) } < 0
        {
            return;
        }
        let mut buf = [0u8; 4096];
        loop {
            // SAFETY: reads into a buffer of the stated length.
            if unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) } <= 0 || !changed() {
                return;
            }
        }
    });
}

impl cxx_qt::Initialize for Conversations {
    fn initialize(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().light = read_light();
        self.as_mut().rust_mut().accent = QString::from(&read_accent());
        // Commands of the fili-bar menu
        let qt = self.qt_thread();
        crate::ctl::serve(move |name| {
            let _ = qt.queue(move |c| c.command(&QString::from(&name)));
        });
        // The settings directory (~/.config/filigrane: fili-im.toml, theme.toml, written by `fili-theme` or by hand)
        let qt = self.qt_thread();
        watch_dir(crate::window_parts::config_dir(), move || {
            qt.queue(move |mut c| {
                let rev = c.settings_rev.wrapping_add(1);
                c.as_mut().rust_mut().settings_rev = rev;
                c.as_mut().theme_changed();
            })
            .is_ok()
        });
        // The state directory (fili-theme light/dark and accent)
        let qt = self.qt_thread();
        if let Some(dir) = theme_file().parent() {
            watch_dir(dir.to_path_buf(), move || {
                let (light, accent) = (read_light(), read_accent());
                qt.queue(move |mut c| {
                    if c.light != light || c.accent.to_string() != accent {
                        c.as_mut().rust_mut().light = light;
                        c.as_mut().rust_mut().accent = QString::from(&accent);
                        c.as_mut().theme_changed();
                    }
                })
                .is_ok()
            });
        }
        // The daemon
        let qt = self.qt_thread();
        std::thread::spawn(move || {
            link::run(move |incoming| {
                let _ = qt.queue(move |mut c| match incoming {
                    Incoming::Link(up) => {
                        c.as_mut().rust_mut().online = up;
                        c.as_mut().online_changed();
                    }
                    Incoming::Reply { id, ok, body } => c.replied(id, ok, &QString::from(&body)),
                    Incoming::Event(text) => c.pushed(&QString::from(&text)),
                });
            })
        });
    }
}

/// The recording in progress: the capture, the encoder, the file.
static VOICE: std::sync::Mutex<Option<(std::process::Child, std::process::Child, String)>> = std::sync::Mutex::new(None);

impl qobject::Conversations {
    pub fn theme_json(&self) -> QString {
        QString::from(&super::theme::theme_json())
    }

    pub fn header(&self) -> bool {
        crate::window_parts::shown_now("header")
    }

    pub fn footer(&self) -> bool {
        crate::window_parts::shown_now("footer")
    }

    pub fn start_voice(self: Pin<&mut Self>, path: &QString) -> bool {
        use std::process::{Command, Stdio};
        let path = path.to_string();
        let mut slot = VOICE.lock().unwrap();
        if slot.is_some() {
            return false;
        }
        let Ok(mut capture) = Command::new("pw-record")
            .args(["--rate", "48000", "--channels", "1", "--format", "s16", "-"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
        else {
            return false;
        };
        let Some(pipe) = capture.stdout.take() else { return false };
        let encoder = Command::new("opusenc")
            .args(["--raw", "--raw-rate", "48000", "--raw-chan", "1", "--raw-bits", "16", "--quiet", "--bitrate", "32", "-"])
            .arg(&path)
            .stdin(Stdio::from(pipe))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        let Ok(encoder) = encoder else {
            let _ = capture.kill();
            let _ = capture.wait();
            return false;
        };
        *slot = Some((capture, encoder, path));
        true
    }

    pub fn stop_voice(self: Pin<&mut Self>) {
        let Some((mut capture, mut encoder, path)) = VOICE.lock().unwrap().take() else { return };
        let qt = self.qt_thread();
        // The capture ends (SIGTERM), the encoder sees the end of its input and finishes the file
        // SAFETY: signalling a child process this function owns.
        unsafe { libc::kill(capture.id() as i32, libc::SIGTERM) };
        std::thread::spawn(move || {
            let _ = capture.wait();
            let ok = encoder.wait().map(|s| s.success()).unwrap_or(false);
            let _ = qt.queue(move |c| c.voice_finished(&QString::from(path.as_str()), ok));
        });
    }

    pub fn voice_path(&self) -> QString {
        let dir = crate::paths::runtime_dir();
        let _ = crate::paths::private_dir(&dir);
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
        QString::from(dir.join(format!("voice-{stamp}.ogg")).display().to_string().as_str())
    }

    /// Only the recordings made here are removed (`voice-*.ogg` in the runtime directory).
    pub fn discard_voice(&self, path: &QString) {
        let path = std::path::PathBuf::from(path.to_string());
        let ours = path.parent() == Some(crate::paths::runtime_dir().as_path())
            && path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("voice-") && n.to_string_lossy().ends_with(".ogg"));
        if ours {
            let _ = std::fs::remove_file(path);
        }
    }

    pub fn preview_link(self: Pin<&mut Self>, url: &QString) -> i32 {
        use std::sync::atomic::{AtomicI32, Ordering};
        static NEXT: AtomicI32 = AtomicI32::new(1);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let url = url.to_string();
        let qt = self.qt_thread();
        std::thread::spawn(move || {
            // The same link in many messages is asked for once (the failures too)
            static CACHE: std::sync::Mutex<Option<std::collections::HashMap<String, String>>> = std::sync::Mutex::new(None);
            let cached = CACHE.lock().unwrap().get_or_insert_with(Default::default).get(&url).cloned();
            let json = cached.unwrap_or_else(|| {
                let json = crate::preview::fetch(&url).map(|v| v.to_string()).unwrap_or_else(|| "{}".to_string());
                CACHE.lock().unwrap().get_or_insert_with(Default::default).insert(url.clone(), json.clone());
                json
            });
            let _ = qt.queue(move |c| c.link_preview(id, &QString::from(json.as_str())));
        });
        id
    }

    pub fn resolve_video(self: Pin<&mut Self>, url: &QString) -> i32 {
        use std::sync::atomic::{AtomicI32, Ordering};
        static NEXT: AtomicI32 = AtomicI32::new(1);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let url = url.to_string();
        let qt = self.qt_thread();
        std::thread::spawn(move || {
            let json = crate::preview::resolve_video(&url).to_string();
            let _ = qt.queue(move |c| c.video_resolved(id, &QString::from(json.as_str())));
        });
        id
    }

    pub fn link_mode(&self) -> QString {
        let text = std::fs::read_to_string(crate::window_parts::config_dir().join("fili-im.toml")).unwrap_or_default();
        let mut in_links = false;
        for line in text.lines().map(str::trim) {
            if line.starts_with('[') {
                in_links = line.starts_with("[links]");
            } else if in_links && line.starts_with("previews") {
                if line.contains("\"off\"") {
                    return QString::from("off");
                }
                if line.contains("\"youtube\"") {
                    return QString::from("youtube");
                }
                return QString::from("all");
            }
        }
        QString::from("all")
    }

    pub fn play_link(&self, url: &QString) -> bool {
        let url = url.to_string();
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            return false;
        }
        std::process::Command::new("mpv")
            .args(["--force-window=yes", "--keep-open=no", "--wayland-app-id=quicklook", "--ytdl-format=bv*[height<=1080]+ba/b"])
            .arg("--")
            .arg(&url)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .is_ok()
    }

    pub fn link_previews(&self) -> bool {
        let text = std::fs::read_to_string(crate::window_parts::config_dir().join("fili-im.toml")).unwrap_or_default();
        let mut in_links = false;
        for line in text.lines().map(str::trim) {
            if line.starts_with('[') {
                in_links = line.starts_with("[links]");
            } else if in_links && line.starts_with("previews") {
                return !line.contains("\"off\"");
            }
        }
        true
    }

    pub fn emojis(&self) -> QString {
        let path = std::env::var("FILI_EMOJI")
            .ok()
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::var("HOME").ok().map(|h| std::path::Path::new(&h).join(".local/share/menu/emoji.txt")));
        QString::from(path.and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default().as_str())
    }

    pub fn ui_get(&self, key: &QString) -> QString {
        let key = key.to_string();
        let text = std::fs::read_to_string(ui_file()).unwrap_or_default();
        let value = text.lines().find_map(|l| l.strip_prefix(&format!("{key}="))).unwrap_or("");
        QString::from(value)
    }

    pub fn logs(&self, since: i32) -> QString {
        let (seq, entries) = crate::logs::since(since.max(0) as u64);
        QString::from(&serde_json::json!({ "seq": seq, "entries": entries }).to_string())
    }

    pub fn log_file(&self) -> QString {
        QString::from(&crate::logs::path().display().to_string())
    }

    pub fn clear_logs(&self) {
        crate::logs::clear();
    }

    pub fn open_path(&self, path: &QString) {
        let _ = std::process::Command::new("xdg-open")
            .arg(path.to_string())
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }

    pub fn report_problem(&self, text: &QString) -> QString {
        let said = text.to_string();
        let message = if said.trim().is_empty() { "Problem reported from the Debug panel".to_string() } else { said };
        QString::from(&crate::logs::incident("manual", &message, true).unwrap_or_default())
    }

    pub fn ui_state(&self, key: &QString) -> QString {
        self.ui_get(key)
    }

    pub fn set_ui_state(&self, key: &QString, value: &QString) {
        self.ui_set(key, value)
    }

    pub fn ui_set(&self, key: &QString, value: &QString) {
        let (key, value) = (key.to_string(), value.to_string());
        let path = ui_file();
        let old = std::fs::read_to_string(&path).unwrap_or_default();
        let mut lines: Vec<String> = old.lines().filter(|l| !l.starts_with(&format!("{key}="))).map(str::to_owned).collect();
        lines.push(format!("{key}={value}"));
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&path, lines.join("\n") + "\n");
    }

    pub fn window_control(&self) -> bool {
        crate::window::available()
    }

    pub fn window_state(&self) -> QString {
        QString::from(&crate::window::state())
    }

    pub fn window_action(&self, name: &QString) -> bool {
        crate::window::run(&name.to_string())
    }

    pub fn tr(&self, text: &QString) -> QString {
        QString::from(&super::i18n::tr(&text.to_string()))
    }

    pub fn call(self: Pin<&mut Self>, method: &QString, params: &QString) -> i32 {
        let (id, sent) = link::link().call(&method.to_string(), &params.to_string());
        if sent.is_err() {
            // Answered at once, from the same place as a real answer, so QML has one path to handle
            let qt = self.qt_thread();
            let body = json!({"code": "offline", "message": "The daemon is not running"}).to_string();
            let _ = qt.queue(move |c| c.replied(id, false, &QString::from(&body)));
        }
        id
    }

    pub fn about_rows(&self) -> QString {
        let t = |s: &str| super::i18n::tr(s);
        let home = home();
        let settings = format!("{home}/.config/filigrane/fili-im.toml").replace(&home, "~");
        let version = match option_env!("FILI_IM_GIT") {
            Some(git) if !git.is_empty() => format!("{} ({git})", env!("CARGO_PKG_VERSION")),
            _ => env!("CARGO_PKG_VERSION").to_string(),
        };
        let rows = json!([
            [t("Version"), version],
            [t("Also known as"), "fili-im"],
            [t("Part of"), "Filigrane"],
            [t("Built with"), "Rust, Qt Quick"],
            [t("Messages"), "~/.local/share/fili-im"],
            [t("Settings"), settings],
        ]);
        QString::from(&rows.to_string())
    }

    pub fn qr_rows(&self, text: &QString) -> QString {
        let rows: Vec<String> = match qrcode::QrCode::new(text.to_string().as_bytes()) {
            Ok(code) => {
                let width = code.width();
                let colors = code.to_colors();
                colors
                    .chunks(width)
                    .map(|row| row.iter().map(|c| if *c == qrcode::Color::Dark { '1' } else { '0' }).collect())
                    .collect()
            }
            Err(_) => Vec::new(),
        };
        QString::from(&json!(rows).to_string())
    }

    pub fn open_url(&self, url: &QString) {
        let url = url.to_string();
        // Only what the user can open on purpose: web links and local files, never an option for xdg-open
        if url.starts_with("https://") || url.starts_with("http://") || url.starts_with("file://") || url.starts_with("mailto:") {
            let _ = std::process::Command::new("xdg-open")
                .arg(url)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();
        }
    }

    pub fn clipboard_files(&self) -> QString {
        let wl = |args: &[&str]| {
            std::process::Command::new("wl-paste")
                .args(args)
                .stdin(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .output()
                .ok()
                .filter(|o| o.status.success())
        };
        let Some(types) = wl(&["--list-types"]) else { return QString::from("[]") };
        let types = String::from_utf8_lossy(&types.stdout).into_owned();
        let mut found: Vec<String> = Vec::new();
        if types.lines().any(|t| t == "text/uri-list")
            && let Some(list) = wl(&["--no-newline", "--type", "text/uri-list"])
        {
            found =
                String::from_utf8_lossy(&list.stdout).lines().filter(|l| l.starts_with("file://")).map(str::to_owned).collect();
        }
        if found.is_empty()
            && types.lines().any(|t| t == "image/png")
            && let Some(png) = wl(&["--type", "image/png"])
        {
            let dir = crate::paths::runtime_dir();
            let _ = crate::paths::private_dir(&dir);
            let path = dir.join(format!("pasted-{}.png", crate::store::now_ms()));
            if std::fs::write(&path, &png.stdout).is_ok() {
                found.push(path.display().to_string());
            }
        }
        QString::from(&json!(found).to_string())
    }

    pub fn copy_text(&self, text: &QString) {
        use std::io::Write;
        if let Ok(mut child) = std::process::Command::new("wl-copy")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            && let Some(mut stdin) = child.stdin.take()
        {
            let _ = stdin.write_all(text.to_string().as_bytes());
        }
    }
}
