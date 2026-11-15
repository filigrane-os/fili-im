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

    pub fn ui_get(&self, key: &QString) -> QString {
        let key = key.to_string();
        let text = std::fs::read_to_string(ui_file()).unwrap_or_default();
        let value = text.lines().find_map(|l| l.strip_prefix(&format!("{key}="))).unwrap_or("");
        QString::from(value)
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
