//! The window of Conversations: Qt Quick interface (qml/) over the daemon's control socket.
pub mod bridge;
pub mod i18n;
pub mod link;
pub mod theme;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QString, QUrl};

/// Open the window; returns the process exit code.
pub fn run() -> i32 {
    // Static initializers of the Qt side, named here so the linker keeps them (the package also has a library crate).
    cxx_qt::init_crate!(cxx_qt_lib);
    cxx_qt::init_crate!(fili_im);
    cxx_qt::init_qml_module!("Fili.Im");
    // Through the desktop portal, "Attach files" opens the desktop's file chooser (Casier on Filigrane). Must be set
    // before the application object exists.
    if std::env::var_os("QT_QPA_PLATFORMTHEME").is_none() {
        // SAFETY: nothing else runs yet (single thread, before Qt starts)
        unsafe { std::env::set_var("QT_QPA_PLATFORMTHEME", "xdgdesktopportal") };
    }
    // The log of the window (Debug panel, View > Debug Logs): ~/.local/state/filigrane/logs/fili-im.log; panics and QML
    // errors become incidents (fili-incident). Nothing of the messages is logged.
    crate::logs::init("fili-im", "filigrane-os/fili-im", env!("CARGO_PKG_VERSION"), crate::window_parts::incidents_on());
    crate::logs::capture_qt();
    let mut app = QGuiApplication::new();
    // Wayland app-id (niri rules, fili-im.desktop)
    QGuiApplication::set_desktop_file_name(&QString::from("fili-im"));
    let mut engine = QQmlApplicationEngine::new();
    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from("qrc:/qt/qml/Fili/Im/qml/Main.qml"));
    }
    match app.as_mut() {
        Some(app) => app.exec(),
        None => 1,
    }
}
